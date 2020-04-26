use rusqlite::{Connection, NO_PARAMS, params};
use std::sync::{Mutex, MutexGuard};


lazy_static! {
    static ref CONN: Mutex<Connection> = Mutex::new(Connection::open("./world/plots.db").expect("Error opening plot database!"));
}

fn lock<'a>() -> MutexGuard<'a, Connection> {
    CONN.lock().unwrap()
}

pub fn get_plot_owner(plot_x: i32, plot_z: i32) -> Option<u128> {
    lock().query_row(
        "SELECT owner FROM plots WHERE plot_x=?1 AND plot_z=?2",
        params![plot_x, plot_z],
        |row| row.get::<_, String>(0)
    ).ok().map(|uuid| uuid.parse().unwrap())
}

pub fn claim_plot(plot_x: i32, plot_z: i32, owner: &str) {
    lock().execute(
        "INSERT INTO plots (plot_x, plot_z, owner) VALUES (?1, ?2, ?3)",
        params![plot_x, plot_z, owner]
    ).unwrap();
}

pub fn init() {
    let conn = lock();

    conn.execute(
        "create table if not exists plots (
            id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            plot_x INTEGER NOT NULL,
            plot_z int NOT NULL,
            owner VARCHAR(40) NOT NULL
        )",
        NO_PARAMS,
    ).unwrap();
}