use rocket::{fairing::AdHoc, fs::FileServer, serde::Serialize, Build, Rocket};
use rocket_dyn_templates::Template;

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_sync_db_pools;
#[macro_use]
extern crate diesel;

mod http;
mod schema;
mod tls;

#[database("sqlite_database")]
pub struct DbConn(diesel::SqliteConnection);

#[launch]
async fn rocket() -> _ {
    rocket::build()
        .mount(
            "/",
            routes![
                index,
                http::index,
                http::create,
                http::update,
                http::enable,
                http::delete,
                tls::index,
                tls::create,
                tls::update,
                tls::enable,
                tls::delete
            ],
        )
        .mount("/static", FileServer::from("templates/static"))
        .attach(Template::fairing())
        .attach(DbConn::fairing())
        .attach(AdHoc::on_ignite("Run Migrations", run_migrations))
}

async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    DbConn::get_one(&rocket)
        .await
        .expect("database connection")
        .run(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .expect("diesel migrations");
        })
        .await;

    rocket
}

#[derive(Serialize)]
struct Index {
    http_count: i64,
    tls_count: i64,
}

#[get("/")]
async fn index(conn: DbConn) -> Template {
    let http_count = http::HttpRoute::count(&conn).await.unwrap_or(0);
    let tls_count = tls::TlsRoute::count(&conn).await.unwrap_or(0);
    Template::render(
        "index",
        &Index {
            http_count,
            tls_count,
        },
    )
}
