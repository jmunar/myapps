use anyhow::Result;
use sqlx::SqlitePool;

pub async fn run(pool: &SqlitePool, user_id: i64) -> Result<()> {
    // Wipe all ClassroomInput data for this user (cascade handles inputs)
    sqlx::query("DELETE FROM classroom_inputs WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM classroom_form_types WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM classroom_classrooms WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    tracing::info!("Cleared existing ClassroomInput data for user {user_id}");

    // ── Classrooms ────────────────────────────────────────────────
    let classrooms: &[(&str, &[&str])] = &[
        (
            "1-A",
            &[
                "Alba García",
                "Carlos López",
                "Diana Martínez",
                "Elena Ruiz",
                "Fernando Sánchez",
                "Gabriela Torres",
                "Hugo Fernández",
                "Irene Moreno",
                "Javier Díaz",
                "Laura Romero",
                "Manuel Navarro",
                "Nuria Jiménez",
                "Óscar Álvarez",
                "Paula Domínguez",
                "Raúl Muñoz",
            ],
        ),
        (
            "1-B",
            &[
                "Adrián Vega",
                "Beatriz Molina",
                "Cristina Herrera",
                "Daniel Ortega",
                "Eva Castro",
                "Francisco Gil",
                "Gloria Ramos",
                "Héctor Serrano",
                "Isabel Blanco",
                "Jorge Medina",
                "Karina Iglesias",
                "Luis Rubio",
                "María Peña",
                "Nicolás Flores",
            ],
        ),
        (
            "2-A",
            &[
                "Alejandro Prieto",
                "Blanca Herrero",
                "Carmen Cano",
                "David Reyes",
                "Emilia Aguilar",
                "Felipe Pascual",
                "Gemma Cortés",
                "Iván Delgado",
                "Julia Fuentes",
                "Kevin Santos",
                "Lucía Cabrera",
                "Mario Campos",
            ],
        ),
    ];

    let mut cls_count = 0u64;
    for (label, pupils) in classrooms {
        let pupils_text = pupils.join("\n");
        let result = sqlx::query(
            "INSERT INTO classroom_classrooms (user_id, label, pupils) VALUES (?, ?, ?)",
        )
        .bind(user_id)
        .bind(label)
        .bind(&pupils_text)
        .execute(pool)
        .await?;
        cls_count += result.rows_affected();
    }
    tracing::info!("Seeded {cls_count} classrooms");

    // ── Form types ────────────────────────────────────────────────
    let form_types: &[(&str, &str)] = &[
        (
            "Weekly quiz",
            r#"[{"name":"Score","type":"number"},{"name":"Comment","type":"text"}]"#,
        ),
        (
            "Attendance",
            r#"[{"name":"Present","type":"bool"},{"name":"Late","type":"bool"},{"name":"Note","type":"text"}]"#,
        ),
        (
            "Reading assessment",
            r#"[{"name":"Fluency","type":"number"},{"name":"Comprehension","type":"number"},{"name":"Vocabulary","type":"number"},{"name":"Overall","type":"text"}]"#,
        ),
        (
            "Behaviour report",
            r#"[{"name":"Participation","type":"number"},{"name":"Respect","type":"number"},{"name":"Effort","type":"number"},{"name":"Remark","type":"text"}]"#,
        ),
    ];

    let mut ft_count = 0u64;
    for (name, columns_json) in form_types {
        let result = sqlx::query(
            "INSERT INTO classroom_form_types (user_id, name, columns_json) VALUES (?, ?, ?)",
        )
        .bind(user_id)
        .bind(name)
        .bind(columns_json)
        .execute(pool)
        .await?;
        ft_count += result.rows_affected();
    }
    tracing::info!("Seeded {ft_count} form types");

    // ── Inputs ────────────────────────────────────────────────────
    let cls_1a = cls_id(pool, user_id, "1-A").await;
    let cls_1b = cls_id(pool, user_id, "1-B").await;
    let cls_2a = cls_id(pool, user_id, "2-A").await;
    let ft_quiz = ft_id(pool, user_id, "Weekly quiz").await;
    let ft_attendance = ft_id(pool, user_id, "Attendance").await;
    let ft_reading = ft_id(pool, user_id, "Reading assessment").await;

    let mut inp_count = 0u64;

    // 1-A: Weekly quiz — Week 10
    if let (Some(cid), Some(fid)) = (cls_1a, ft_quiz) {
        let csv = "\
Pupil,Score,Comment
Alba García,8.5,Good improvement
Carlos López,7,Needs more practice with fractions
Diana Martínez,9.5,Excellent
Elena Ruiz,6,Struggled with word problems
Fernando Sánchez,8,
Gabriela Torres,9,Very thorough answers
Hugo Fernández,5.5,Absent last week — review needed
Irene Moreno,8,
Javier Díaz,7.5,
Laura Romero,9,
Manuel Navarro,6.5,
Nuria Jiménez,8.5,
Óscar Álvarez,7,Improving steadily
Paula Domínguez,9,
Raúl Muñoz,7.5,";
        inp_count += insert_input(pool, user_id, cid, fid, "Week 10 quiz", csv).await;
    }

    // 1-A: Weekly quiz — Week 11
    if let (Some(cid), Some(fid)) = (cls_1a, ft_quiz) {
        let csv = "\
Pupil,Score,Comment
Alba García,9,
Carlos López,7.5,Better this week
Diana Martínez,10,Perfect score
Elena Ruiz,7,Good progress
Fernando Sánchez,8.5,
Gabriela Torres,8,
Hugo Fernández,7,Caught up well
Irene Moreno,8.5,
Javier Díaz,8,
Laura Romero,9.5,
Manuel Navarro,7,
Nuria Jiménez,9,Great work
Óscar Álvarez,7.5,
Paula Domínguez,8.5,
Raúl Muñoz,8,";
        inp_count += insert_input(pool, user_id, cid, fid, "Week 11 quiz", csv).await;
    }

    // 1-B: Attendance — Monday March 10
    if let (Some(cid), Some(fid)) = (cls_1b, ft_attendance) {
        let csv = "\
Pupil,Present,Late,Note
Adrián Vega,Yes,No,
Beatriz Molina,Yes,No,
Cristina Herrera,No,No,Sick — flu
Daniel Ortega,Yes,Yes,Arrived 10 min late
Eva Castro,Yes,No,
Francisco Gil,Yes,No,
Gloria Ramos,Yes,No,
Héctor Serrano,No,No,Family trip
Isabel Blanco,Yes,No,
Jorge Medina,Yes,No,
Karina Iglesias,Yes,Yes,Bus delay
Luis Rubio,Yes,No,
María Peña,Yes,No,
Nicolás Flores,Yes,No,";
        inp_count += insert_input(pool, user_id, cid, fid, "Attendance — Mon 10 Mar", csv).await;
    }

    // 2-A: Reading assessment — March
    if let (Some(cid), Some(fid)) = (cls_2a, ft_reading) {
        let csv = "\
Pupil,Fluency,Comprehension,Vocabulary,Overall
Alejandro Prieto,7,8,7,Good
Blanca Herrero,9,9,8,Excellent
Carmen Cano,6,7,6,Satisfactory
David Reyes,8,7,8,Good
Emilia Aguilar,9,9,9,Outstanding
Felipe Pascual,5,6,5,Needs support
Gemma Cortés,7,8,7,Good
Iván Delgado,6,5,6,Below expectations
Julia Fuentes,8,8,7,Good
Kevin Santos,7,7,7,Satisfactory
Lucía Cabrera,9,8,9,Excellent
Mario Campos,6,6,5,Needs improvement";
        inp_count += insert_input(pool, user_id, cid, fid, "Reading assessment — March", csv).await;
    }

    tracing::info!("Seeded {inp_count} inputs");
    tracing::info!("ClassroomInput seed complete");

    Ok(())
}

async fn cls_id(pool: &SqlitePool, user_id: i64, label: &str) -> Option<i64> {
    sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM classroom_classrooms WHERE user_id = ? AND label = ?",
    )
    .bind(user_id)
    .bind(label)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|r| r.0)
}

async fn ft_id(pool: &SqlitePool, user_id: i64, name: &str) -> Option<i64> {
    sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM classroom_form_types WHERE user_id = ? AND name = ?",
    )
    .bind(user_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|r| r.0)
}

async fn insert_input(
    pool: &SqlitePool,
    user_id: i64,
    classroom_id: i64,
    form_type_id: i64,
    name: &str,
    csv: &str,
) -> u64 {
    sqlx::query(
        "INSERT INTO classroom_inputs (user_id, classroom_id, form_type_id, name, csv_data) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(classroom_id)
    .bind(form_type_id)
    .bind(name)
    .bind(csv)
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0)
}
