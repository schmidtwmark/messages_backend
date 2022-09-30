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
        name TEXT NOT NULL,
        password TEXT
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
    password: Option<String>
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
    password: Option<String>
}

#[derive(Serialize, Deserialize)]
enum GetUserError {
    NotFound,
    WrongPassword
}

async fn create_user(connection: &mut SqliteConnection, name: &String, password: &Option<String>) -> Result<(), sqlx::Error> {
    query("INSERT INTO users (name, password) VALUES (?, ?)")
        .bind(name)
        .bind(password)
        .execute(connection)
        .await?;

    Ok(())
}

async fn get_user(connection: &mut SqliteConnection, name: &str, password: &Option<String>, require_password_match: bool) -> Result<User, GetUserError> {
    let user: User = sqlx::query_as("SELECT * FROM users WHERE name = ?")
        .bind(name)
        .fetch_one(connection)
        .await.map_err(|_| GetUserError::NotFound)?;
    
    if require_password_match {
        if user.password == *password {
            Ok(user)
        } else {
            Err(GetUserError::WrongPassword)
        }
    } else {
        Ok(user)
    }

}

async fn get_or_create_user(conn: &mut SqliteConnection, name: &String, password: &Option<String>) -> Result<User, GetUserError> {
    match get_user(conn, name, password, true).await {
        Ok(user) => Ok(user),
        Err(GetUserError::NotFound) => 
        {
            create_user(conn, name, password).await.map_err(|_| GetUserError::WrongPassword)?;
            Ok(get_user(conn, name, password, true).await.map_err(|_| (GetUserError::WrongPassword))?)
        },
        Err(GetUserError::WrongPassword) => Err(GetUserError::WrongPassword)
    }


}


async fn get_messages(Json(payload): Json<MessagesRequest>) -> Result<Json<Vec<Message>>, (StatusCode, String)>{  
    tracing::info!("Got request for messages for {}", payload.target);

    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let user = get_or_create_user(&mut conn, &payload.target, &payload.password).await.map_err(|_| (StatusCode::UNAUTHORIZED, "Failed to get user".to_string()))?;
    

    let messages = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE target = ?")
     .bind(user.id)
     .fetch_all(&mut conn).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error while fetching messages {e}")))?;


    Ok(Json(messages))
} 

async fn send_message(Json(payload): Json<IncomingMessage>) -> Result<StatusCode, (StatusCode, String)> {
    tracing::info!("Sending message {:?}", payload);
    let mut conn = SqliteConnection::connect("messages.db").await.unwrap();

    // Get user id
    let author = get_user(&mut conn, &payload.author, &payload.password, true).await.map_err(|_| (StatusCode::NOT_FOUND, "User not found".to_string()))?;
    let target = get_user(&mut conn, &payload.target, &None, false).await.map_err(|_| (StatusCode::NOT_FOUND, "Target not found".to_string()))?;

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