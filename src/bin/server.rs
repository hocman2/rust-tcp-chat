mod chat_message;

use std::{net::SocketAddr, sync::Arc};

use tokio::{io::{AsyncReadExt, AsyncWriteExt, WriteHalf}, net::{TcpListener, TcpStream}};
use tokio::sync::{mpsc, Mutex};
use tokio::sync::mpsc::Receiver;
use chat_message::ChatMessage;

type SocketWtsVec = Arc<Mutex<Vec<(SocketAddr, WriteHalf<TcpStream>)>>>;

#[tokio::main]
async fn main() {
    let server_address = "127.0.0.1:6969";
    let listener = TcpListener::bind(server_address).await.unwrap();
    
    // This will hold the write half of every opened sockets
    let wt_sockets: SocketWtsVec = Arc::new(Mutex::new(Vec::new()));

    println!("Listening on {}", server_address);

    // Create a channel where every message received from clients will be transmitted to the broadcast message task
    let (tx, rx) = mpsc::channel(32);

    // We must create the clone outside the spawned task
    let wt_sockets_broadcast = wt_sockets.clone();
    tokio::spawn(async move {
        broadcast_messages(rx, wt_sockets_broadcast).await;
    });

    // Loop and accept an undefined amount of connections, normally this should be hard bounded to the max number in the channel (32)
    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        
        println!("New user connected: {:?}", addr);

        // Split the socket, keep the write portion somewhere else and use the read portion to receive messages
        let (mut socket_rd, socket_wt) = tokio::io::split(socket);
        wt_sockets.lock().await.push((addr.clone(), socket_wt));

        // Clone that arc and pass it to the new task
        let wt_sockets = wt_sockets.clone();
        // Create a new channel sender, shadow the original one with a clone
        let tx = tx.clone();
        tokio::spawn(async move {
            // This user can send an undefined amount of messages
            loop {
                // Receive message in the buffer
                let mut buff = vec![0; 1024];
                match socket_rd.read(&mut buff).await {
                    Ok(num_bytes) => {
                        if num_bytes > 0 {
                            // Format and send the data through the channel
                            let message_packet = String::from_utf8_lossy(&buff[..num_bytes]).to_string();
                            let chat_message = ChatMessage::from(message_packet);
                            tx.send((addr, chat_message)).await.unwrap();
                        }
                    },
                    Err(_) => {
                        println!("Connection closed with {:?}", addr);
                        // Remove that socket from the list
                        let mut wt_sockets = wt_sockets.lock().await;
                        let idx = wt_sockets.iter().position(|e| e.0 == addr);
                        if let Some(idx) = idx {
                            println!("Removing idx #{}", idx);
                            wt_sockets.remove(idx);
                        }
                        break
                    }
                }
            }
            println!("Killing task for {:?}", addr);
        });
    }
}

// Broadcast every received message
async fn broadcast_messages(mut rx: Receiver<(SocketAddr, ChatMessage)>, wt_sockets: SocketWtsVec) {
    // Start listening to received messages
    while let Some((sender_addr, message)) = rx.recv().await {

        // Display some server side info
        println!("Received message from {} => {}", message.username, message.content);

        // Hold the write part of every opened sockets
        let mut wt_sockets = wt_sockets.lock().await;

        // Send back the message, except to the original sender
        for (addr, wt) in wt_sockets.iter_mut() {
            if *addr != sender_addr {
                wt.write(message.to_string().as_bytes()).await.unwrap();
                wt.flush().await.unwrap();
            }
        }
    }
}