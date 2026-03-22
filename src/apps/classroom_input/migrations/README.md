# ClassroomInput Database Schema

### classroom_input_classrooms

| Column     | Type    | Notes                          |
|------------|---------|--------------------------------|
| id         | INTEGER | PK, autoincrement              |
| user_id    | INTEGER | FK → users                     |
| label      | TEXT    | NOT NULL (e.g. "1-A")          |
| pupils     | TEXT    | Newline-separated pupil names  |
| created_at | TEXT    | ISO 8601                       |

### classroom_input_form_types

| Column       | Type    | Notes                                          |
|--------------|---------|-------------------------------------------------|
| id           | INTEGER | PK, autoincrement                              |
| user_id      | INTEGER | FK → users                                     |
| name         | TEXT    | NOT NULL                                       |
| columns_json | TEXT    | JSON array: `[{"name":"…","type":"text\|number\|bool"}]` |
| created_at   | TEXT    | ISO 8601                                       |
| updated_at   | TEXT    | ISO 8601                                       |

### classroom_input_inputs

| Column       | Type    | Notes                                 |
|--------------|---------|---------------------------------------|
| id           | INTEGER | PK, autoincrement                     |
| user_id      | INTEGER | FK → users                            |
| classroom_id | INTEGER | FK → classroom_input_classrooms       |
| form_type_id | INTEGER | FK → classroom_input_form_types       |
| name         | TEXT    | NOT NULL                              |
| csv_data     | TEXT    | Raw CSV (header + one row per pupil)  |
| created_at   | TEXT    | ISO 8601                              |
