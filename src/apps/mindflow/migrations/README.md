# MindFlow Database Schema

### mindflow_categories

| Column     | Type    | Notes                                  |
|------------|---------|----------------------------------------|
| id         | INTEGER | PK, autoincrement                      |
| user_id    | INTEGER | FK → users                             |
| name       | TEXT    | NOT NULL, UNIQUE(user_id, name)        |
| color      | TEXT    | NOT NULL, default '#6B6B6B'            |
| icon       | TEXT    | Nullable                               |
| parent_id  | INTEGER | Nullable FK → mindflow_categories      |
| archived   | INTEGER | 0 or 1, default 0                      |
| position   | INTEGER | Ordering, default 0                    |
| created_at | TEXT    | ISO 8601                               |

### mindflow_thoughts

| Column            | Type    | Notes                                 |
|-------------------|---------|---------------------------------------|
| id                | INTEGER | PK, autoincrement                     |
| user_id           | INTEGER | FK → users                            |
| category_id       | INTEGER | Nullable FK → mindflow_categories     |
| parent_thought_id | INTEGER | Nullable FK → mindflow_thoughts (nesting) |
| content           | TEXT    | NOT NULL                              |
| status            | TEXT    | 'active' or 'archived'                |
| created_at        | TEXT    | ISO 8601                              |
| updated_at        | TEXT    | ISO 8601                              |

### mindflow_comments

| Column     | Type    | Notes                                  |
|------------|---------|----------------------------------------|
| id         | INTEGER | PK, autoincrement                      |
| thought_id | INTEGER | FK → mindflow_thoughts, ON DELETE CASCADE |
| content    | TEXT    | NOT NULL                               |
| created_at | TEXT    | ISO 8601                               |

### mindflow_actions

| Column       | Type    | Notes                                |
|--------------|---------|--------------------------------------|
| id           | INTEGER | PK, autoincrement                    |
| thought_id   | INTEGER | FK → mindflow_thoughts, ON DELETE CASCADE |
| user_id      | INTEGER | FK → users                           |
| title        | TEXT    | NOT NULL                             |
| due_date     | TEXT    | Nullable, ISO 8601 date              |
| priority     | TEXT    | 'low', 'medium', 'high'             |
| status       | TEXT    | 'pending' or 'done'                  |
| created_at   | TEXT    | ISO 8601                             |
| completed_at | TEXT    | Nullable, set when status → done     |
