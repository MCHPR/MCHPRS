use once_cell::sync::Lazy;
use rusqlite::{params, Connection};
use std::sync::{Mutex, MutexGuard};

static CONN: Lazy<Mutex<Connection>> = Lazy::new(|| {
    Mutex::new(Connection::open("./world/plots.db").expect("Error opening plot database!"))
});

fn lock<'a>() -> MutexGuard<'a, Connection> {
    CONN.lock().unwrap()
}

pub fn get_plot_owner(plot_x: i32, plot_z: i32) -> Option<String> {
    lock()
        .query_row(
            "SELECT
                uuid
            FROM
                plot
            JOIN
                userplot ON userplot.plot_id = plot.id
            JOIN
                user ON user.id = userplot.user_id
            WHERE
                plot_x=?1
                AND plot_z=?2
                AND is_owner=TRUE",
            params![plot_x, plot_z],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

pub fn get_cached_username(uuid: String) -> Option<String> {
    lock()
        .query_row(
            "SELECT
                name
            FROM
                user
            WHERE
                uuid=?1",
            params![uuid],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

pub fn get_owned_plots(player: &str) -> Vec<(i32, i32)> {
    let conn = lock();
    let mut stmt = conn
        .prepare_cached(
            "SELECT
                    plot_x, plot_z
                FROM
                    plot
                JOIN
                    userplot ON userplot.plot_id = plot.id
                JOIN
                    user ON user.id = userplot.user_id
                WHERE
                    name=?1
                    AND is_owner=TRUE",
        )
        .unwrap();
    stmt.query_map(params![player], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

pub fn is_claimed(plot_x: i32, plot_z: i32) -> Option<bool> {
    lock()
        .query_row(
            "SELECT EXISTS(SELECT * FROM plot WHERE plot_x = ?1 AND plot_z = ?2)",
            params![plot_x, plot_z],
            |row| row.get::<_, bool>(0),
        )
        .ok()
}

pub fn claim_plot(plot_x: i32, plot_z: i32, uuid: &str) {
    let conn = lock();
    conn.execute(
        "INSERT INTO plot(plot_x, plot_z) VALUES(?1, ?2)",
        params![plot_x, plot_z],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO userplot(user_id, plot_id, is_owner)
                VALUES(
                    (SELECT id FROM user WHERE user.uuid = ?1),
                    LAST_INSERT_ROWID(),
                    TRUE
                )",
        params![uuid],
    )
    .unwrap();
}

pub fn ensure_user(uuid: &str, name: &str) {
    lock()
        .execute(
            "INSERT INTO user(uuid, name)
                VALUES (?1, ?2)
                ON CONFLICT (uuid) DO UPDATE SET name = ?3",
            params![uuid, name, name],
        )
        .unwrap();
}

pub fn init() {
    let conn = lock();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS user(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid BLOB(16) UNIQUE NOT NULL,
            name VARCHAR(16) NOT NULL
        )",
        [],
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS plot(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plot_x INTEGER NOT NULL,
            plot_z INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS userplot(
            user_id INTEGER NOT NULL,
            plot_id INTEGER NOT NULL,
            is_owner BOOLEAN NOT NULL DEFAULT FALSE,
            FOREIGN KEY(user_id) REFERENCES user(id),
            FOREIGN KEY(plot_id) REFERENCES plot(id)
        )",
        [],
    )
    .unwrap();
}
