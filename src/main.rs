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
        create_at DATETIME NOT NULL
    )").execute(&mut connection).await?;


    let app = Router::new()
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
    password: Option<String>,
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
struct MessagesRequest {
    target: String,
}


async fn create_user(connection: &mut SqliteConnection, name: &String) -> Result<(), sqlx::Error> {
    query("INSERT INTO users (name, password) VALUES (?, ?)")
        .bind(name)
        .execute(connection)
        .await?;

    Ok(())
}

async fn get_user(connection: &mut SqliteConnection, name: &str) -> Result<User, sqlx::Error> {
    Ok(sqlx::query_as("SELECT * FROM users WHERE name = ?")
        .bind(name)
        .fetch_one(connection)
        .await?)
    

}

async fn get_or_create_user(conn: &mut SqliteConnection, name: &String) -> Result<User, sqlx::Error> {
    match get_user(conn, name).await {
        Ok(user) => Ok(user),
        Err(_) => {
            create_user(conn, name).await?;
            Ok(get_user(conn, name).await?)
        }
    }


}


async fn get_messages(Json(payload): Json<MessagesRequest>) -> Result<Json<Vec<Message>>, (StatusCode, String)>{  
    tracing::info!("Got request for messages for {}", payload.target);

    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let user = get_or_create_user(&mut conn, &payload.target).await.map_err(|_| (StatusCode::UNAUTHORIZED, "Failed to get user".to_string()))?;
    

    let messages = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE target = ?")
     .bind(user.id)
     .fetch_all(&mut conn).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error while fetching messages {e}")))?;


    Ok(Json(messages))
} 

async fn send_message(Json(payload): Json<IncomingMessage>) -> Result<StatusCode, (StatusCode, String)> {
    tracing::info!("Sending message {:?}", payload);
    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let author = get_or_create_user(&mut conn, &payload.author).await.map_err(|_| (StatusCode::NOT_FOUND, "User not found".to_string()))?;
    let target = get_or_create_user(&mut conn, &payload.target).await.map_err(|_| (StatusCode::NOT_FOUND, "Target not found".to_string()))?;

    let message = Message::from(payload);
    let result = sqlx::query("INSERT INTO messages (author, target, text, create_at) VALUES (?, ?, ?, ?)")
        .bind(author.id)
        .bind(target.id)
        .bind(message.text)
        .bind(message.timestamp)
        .execute(&mut conn).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error while inserting message: {e}")))?;
    
    tracing::info!("Inserted message with id {} and result {:?}", result.last_insert_rowid(), result);

    
    Ok(StatusCode::OK)
}