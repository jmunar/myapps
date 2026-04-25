use anyhow::Result;
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;

    // ── Row sets ──────────────────────────────────────────────────
    let row_sets: &[(&str, &[&str])] = &[
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

    let mut rs_count = 0u64;
    for (label, rows) in row_sets {
        let rows_text = rows.join("\n");
        let result =
            sqlx::query("INSERT INTO form_input_row_sets (user_id, label, rows) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(label)
                .bind(&rows_text)
                .execute(pool)
                .await?;
        rs_count += result.rows_affected();
    }
    tracing::info!("Seeded {rs_count} row sets");

    // ── Form types ────────────────────────────────────────────────
    // (name, columns_json, fixed_rows)
    let form_types: &[(&str, &str, bool)] = &[
        (
            "Weekly quiz",
            r#"[{"name":"Score","type":"number"},{"name":"Comment","type":"text"}]"#,
            true,
        ),
        (
            "Attendance",
            r#"[{"name":"Present","type":"bool"},{"name":"Late","type":"bool"},{"name":"Note","type":"text"}]"#,
            true,
        ),
        (
            "Reading assessment",
            r#"[{"name":"Fluency","type":"number"},{"name":"Comprehension","type":"number"},{"name":"Vocabulary","type":"number"},{"name":"Overall","type":"text"}]"#,
            true,
        ),
        (
            "Behaviour report",
            r#"[{"name":"Participation","type":"number"},{"name":"Respect","type":"number"},{"name":"Effort","type":"number"},{"name":"Remark","type":"text"}]"#,
            true,
        ),
        (
            "Expense log",
            r#"[{"name":"Item","type":"text"},{"name":"Amount","type":"number"},{"name":"Reimbursable","type":"bool"},{"name":"Notes","type":"text"}]"#,
            false,
        ),
    ];

    let mut ft_count = 0u64;
    for (name, columns_json, fixed_rows) in form_types {
        let result = sqlx::query(
            "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(name)
        .bind(columns_json)
        .bind(fixed_rows)
        .execute(pool)
        .await?;
        ft_count += result.rows_affected();
    }
    tracing::info!("Seeded {ft_count} form types");

    // ── Inputs ────────────────────────────────────────────────────
    let rs_1a = rs_id(pool, user_id, "1-A").await;
    let rs_1b = rs_id(pool, user_id, "1-B").await;
    let rs_2a = rs_id(pool, user_id, "2-A").await;
    let ft_quiz = ft_id(pool, user_id, "Weekly quiz").await;
    let ft_attendance = ft_id(pool, user_id, "Attendance").await;
    let ft_reading = ft_id(pool, user_id, "Reading assessment").await;

    let mut inp_count = 0u64;

    if let (Some(rsid), Some(fid)) = (rs_1a, ft_quiz) {
        let csv = "\
Row,Score,Comment
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
        inp_count += insert_input(pool, user_id, Some(rsid), fid, "Week 10 quiz", csv).await;
    }

    if let (Some(rsid), Some(fid)) = (rs_1a, ft_quiz) {
        let csv = "\
Row,Score,Comment
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
        inp_count += insert_input(pool, user_id, Some(rsid), fid, "Week 11 quiz", csv).await;
    }

    if let (Some(rsid), Some(fid)) = (rs_1b, ft_attendance) {
        let csv = "\
Row,Present,Late,Note
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
        inp_count += insert_input(
            pool,
            user_id,
            Some(rsid),
            fid,
            "Attendance — Mon 10 Mar",
            csv,
        )
        .await;
    }

    if let (Some(rsid), Some(fid)) = (rs_2a, ft_reading) {
        let csv = "\
Row,Fluency,Comprehension,Vocabulary,Overall
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
        inp_count += insert_input(
            pool,
            user_id,
            Some(rsid),
            fid,
            "Reading assessment — March",
            csv,
        )
        .await;
    }

    // Dynamic-mode example: an expense log with no row set.
    if let Some(fid) = ft_id(pool, user_id, "Expense log").await {
        let csv = "\
Item,Amount,Reimbursable,Notes
Train ticket,42.50,Yes,Client visit
Coffee,3.20,No,
Office supplies,18.75,Yes,Notebooks and pens
Lunch,12.00,Yes,Working session";
        inp_count += insert_input(pool, user_id, None, fid, "March expenses", csv).await;
    }

    tracing::info!("Seeded {inp_count} inputs");
    tracing::info!("FormInput seed complete");

    Ok(())
}

async fn rs_id(pool: &SqlitePool, user_id: i64, label: &str) -> Option<i64> {
    sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM form_input_row_sets WHERE user_id = ? AND label = ?",
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
        "SELECT id FROM form_input_form_types WHERE user_id = ? AND name = ?",
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
    row_set_id: Option<i64>,
    form_type_id: i64,
    name: &str,
    csv: &str,
) -> u64 {
    sqlx::query(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(row_set_id)
    .bind(form_type_id)
    .bind(name)
    .bind(csv)
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0)
}
