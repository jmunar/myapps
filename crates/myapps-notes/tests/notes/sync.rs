use axum_test::WsMessage;
use myapps_notes::NotesApp;
use yrs::sync::{Message as SyncWrapper, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, StateVector, Text, Transact, Update};

async fn app() -> myapps_test_harness::TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(NotesApp::new())]).await
}

fn encode_step1(sv: StateVector) -> Vec<u8> {
    SyncWrapper::Sync(SyncMessage::SyncStep1(sv)).encode_v1()
}

fn encode_update(update: Vec<u8>) -> Vec<u8> {
    SyncWrapper::Sync(SyncMessage::Update(update)).encode_v1()
}

fn decode(bytes: &[u8]) -> SyncWrapper {
    SyncWrapper::decode_v1(bytes).expect("decode sync message")
}

#[tokio::test]
async fn ws_initial_doc_is_empty_when_no_updates_persisted() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let uuid = "abcdef0123456789abcdef0123456789";
    // Insert a note with legacy markdown body. The server must NOT seed this
    // into the CRDT — Tiptap's collaboration extension expects the field to
    // be a Y.XmlFragment and would crash on a Y.Text seeded by the server.
    sqlx::query(
        "INSERT INTO notes_notes (user_id, client_uuid, title, body) VALUES (1, ?, 'Seed', 'Hello world')",
    )
    .bind(uuid)
    .execute(&app.pool)
    .await
    .unwrap();

    let mut ws = app
        .server
        .get_websocket(&format!("/notes/{uuid}/ws"))
        .expect_failure()
        .await
        .into_websocket()
        .await;

    // Discard server's initial SyncStep1.
    let _ = ws.receive_bytes().await;

    ws.send_message(WsMessage::Binary(
        encode_step1(StateVector::default()).into(),
    ))
    .await;

    let bytes = ws.receive_bytes().await;
    let SyncWrapper::Sync(SyncMessage::SyncStep2(diff)) = decode(&bytes) else {
        panic!("expected SyncStep2, got {:?}", decode(&bytes));
    };

    // Empty doc → empty diff (no operations to ship).
    assert!(
        diff.is_empty() || diff == vec![0, 0],
        "expected empty diff, got {diff:?}"
    );

    // And the persisted update log is still empty.
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notes_note_updates WHERE note_id = (SELECT id FROM notes_notes WHERE client_uuid = ?)",
    )
    .bind(uuid)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn ws_two_clients_converge_on_same_note() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let uuid = "11111111111111111111111111111111";
    sqlx::query(
        "INSERT INTO notes_notes (user_id, client_uuid, title, body) VALUES (1, ?, 'Conv', '')",
    )
    .bind(uuid)
    .execute(&app.pool)
    .await
    .unwrap();

    let mut a = app
        .server
        .get_websocket(&format!("/notes/{uuid}/ws"))
        .expect_failure()
        .await
        .into_websocket()
        .await;
    let _ = a.receive_bytes().await;

    let mut b = app
        .server
        .get_websocket(&format!("/notes/{uuid}/ws"))
        .expect_failure()
        .await
        .into_websocket()
        .await;
    let _ = b.receive_bytes().await;

    // Peer A produces an edit locally, encodes the diff, and pushes it.
    let a_doc = Doc::new();
    let a_text = a_doc.get_or_insert_text("body");
    let update_bytes = {
        let mut txn = a_doc.transact_mut();
        a_text.insert(&mut txn, 0, "from-A");
        txn.encode_update_v1()
    };
    a.send_message(WsMessage::Binary(encode_update(update_bytes).into()))
        .await;

    // Peer B receives the broadcast Update.
    let bytes = b.receive_bytes().await;
    let SyncWrapper::Sync(SyncMessage::Update(received)) = decode(&bytes) else {
        panic!("expected Update on B, got {:?}", decode(&bytes));
    };

    let b_doc = Doc::new();
    let b_text = b_doc.get_or_insert_text("body");
    {
        let mut txn = b_doc.transact_mut();
        txn.apply_update(Update::decode_v1(&received).unwrap())
            .unwrap();
    }
    assert_eq!(b_text.get_string(&b_doc.transact()), "from-A");

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notes_note_updates WHERE note_id = (SELECT id FROM notes_notes WHERE client_uuid = ?)",
    )
    .bind(uuid)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        count.0 >= 1,
        "expected at least one persisted update, got {}",
        count.0
    );
}

#[tokio::test]
async fn ws_rejects_unknown_uuid() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let resp = app
        .server
        .get_websocket("/notes/00000000000000000000000000000000/ws")
        .expect_failure()
        .await;
    assert_eq!(resp.status_code(), 403);
}

#[tokio::test]
async fn ws_rejects_unauthenticated() {
    let app = app().await;
    // No login.
    let resp = app
        .server
        .get_websocket("/notes/00000000000000000000000000000000/ws")
        .expect_failure()
        .await;
    // The auth middleware redirects unauthenticated requests to /login.
    assert_eq!(resp.status_code(), 303);
}
