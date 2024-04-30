#[derive(Default, Clone)]
pub struct ChatMessage {
    pub username: String,
    pub content: String,
}

impl From<String> for ChatMessage {
    fn from(value: String) -> Self {
        let value: Vec<&str> = value.split(':').collect();
        
        if value.len() == 2 {
            ChatMessage {
                username: value[0].to_string(),
                content: value[1].to_string()
            }
        // Verification is pretty weak
        } else {
            ChatMessage::default()
        }

    }
}

impl ToString for ChatMessage {
    fn to_string(&self) -> String {
        format!("{}: {}", self.username, self.content)
    }
}