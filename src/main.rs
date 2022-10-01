use axum::{
    http::StatusCode,
    routing::get,
    Json, Router
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::error::Error;
use sqlx::{SqliteConnection, Connection, query};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{
    // init tracing
    tracing_subscriber::fmt::init();

    let mut connection = SqliteConnection::connect("messages.db").await?;

    query("CREATE TABLE IF NOT EXISTS users (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL
    )").execute(&mut connection).await?;
    
    query("CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        author TEXT NOT NULL,
        target TEXT NOT NULL,
        text TEXT NOT NULL,
        timestamp DATETIME NOT NULL
    )").execute(&mut connection).await?;


    let app = Router::new()
        .route("/inbox", get(get_inbox))
        .route("/messages", get(get_messages))
        .route("/send", get(send_message));
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();    println!("Hello, world!");

    Ok(())
}

#[derive(Deserialize, Clone, Debug)]
struct IncomingMessage {
    author: String,
    target: String,
    text: String,
}

#[derive(Serialize, Clone, Debug, sqlx::FromRow)]
struct Message {
    id: i32,
    author: String,
    target: String,
    text: String,
    timestamp: DateTime<Utc>
}

#[derive(Serialize, Clone, Debug, sqlx::FromRow)]
struct User {
    id: i32,
    name: String,
}

impl From<IncomingMessage> for Message {
    fn from(incoming: IncomingMessage) -> Self {
        Message {
            id: 0,
            author: incoming.author,
            text: incoming.text,
            target: incoming.target,
            timestamp: Utc::now()
        }
    }
}

#[derive(Serialize, Deserialize)] 
struct InboxRequest {
    target: String,
}

#[derive(Serialize, Deserialize, Debug)] 
struct MessagesRequest {
    me: String,
    other: String
}


async fn create_user(connection: &mut SqliteConnection, name: &String) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    query("INSERT INTO users (name) VALUES (?)")
        .bind(name)
        .execute(connection)
        .await
}

async fn get_user(connection: &mut SqliteConnection, name: &str) -> Result<User, sqlx::Error> {
    sqlx::query_as("SELECT * FROM users WHERE name = ?")
        .bind(name)
        .fetch_one(connection)
        .await
    

}

async fn get_or_create_user(conn: &mut SqliteConnection, name: &String) -> Result<User, sqlx::Error> {
    match get_user(conn, name).await {
        Ok(user) => Ok(user),
        Err(_) => {
            let result = create_user(conn, name).await?;
            tracing::info!("Created user {} with result {:?}", name, result );
            Ok(get_user(conn, name).await?)
        }
    }
}

async fn get_inbox(Json(payload): Json<InboxRequest>) -> Result<Json<Vec<Message>>, (StatusCode, String)>{  
    tracing::info!("Got request for inbox for {}", payload.target);

    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let user = get_or_create_user(&mut conn, &payload.target).await.map_err(|e| (StatusCode::NOT_FOUND, format!("User not found, error {e}")))?;
    

    let messages = sqlx::query_as::<_, Message>("SELECT messages.text, messages.timestamp, author_name AS author, target_name as target, messages.id FROM messages INNER JOIN (SELECT name AS author_name, id AS author_id FROM users) ON messages.author = author_id INNER JOIN (SELECT name AS target_name, id AS target_id FROM users) ON messages.target = target_id WHERE target_id = ? ORDER BY timestamp")
        .bind(user.id)
        .fetch_all(&mut conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error getting messages: {}", e)))?;


    Ok(Json(messages))

}

async fn get_messages(Json(payload): Json<MessagesRequest>) -> Result<Json<Vec<Message>>, (StatusCode, String)>{  
    tracing::info!("Got request for messages for {:?}", payload);

    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let me = get_or_create_user(&mut conn, &payload.me).await.map_err(|e| (StatusCode::NOT_FOUND, format!("User {} not found, error {e}", payload.me)))?;
    let other = get_or_create_user(&mut conn, &payload.other).await.map_err(|e| (StatusCode::NOT_FOUND, format!("User {} not found, error {e}", payload.other)))?;
    

    let messages = sqlx::query_as::<_, Message>("SELECT messages.text, messages.timestamp, author_name AS author, target_name as target, messages.id FROM messages INNER JOIN (SELECT name AS author_name, id AS author_id FROM users) ON messages.author = author_id INNER JOIN (SELECT name AS target_name, id AS target_id FROM users) ON messages.target = target_id WHERE (author_id = ? or author_id = ?) AND (target_id = ? or target_id = ?) ORDER BY timestamp;")
        .bind(me.id)
        .bind(other.id)
        .bind(me.id)
        .bind(other.id)
        .fetch_all(&mut conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error getting messages: {}", e)))?;


    Ok(Json(messages))
} 

async fn send_message(Json(payload): Json<IncomingMessage>) -> Result<StatusCode, (StatusCode, String)> {
    tracing::info!("Sending message {:?}", payload);
    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let author = get_or_create_user(&mut conn, &payload.author).await.map_err(|e| (StatusCode::NOT_FOUND, format!("User not found, error {e}")))?;
    let target = get_or_create_user(&mut conn, &payload.target).await.map_err(|e| (StatusCode::NOT_FOUND, format!("Target not found {e}")))?;

    let message = Message::from(payload);
    let result = sqlx::query("INSERT INTO messages (author, target, text, timestamp) VALUES (?, ?, ?, ?)")
        .bind(author.id)
        .bind(target.id)
        .bind(message.text)
        .bind(message.timestamp)
        .execute(&mut conn).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error while inserting message: {e}")))?;
    
    tracing::info!("Inserted message with id {} and result {:?}", result.last_insert_rowid(), result);

    
    Ok(StatusCode::OK)
}