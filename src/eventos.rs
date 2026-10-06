use serde::Deserialize;
use std::time::Instant;

#[derive(Clone, Deserialize)]
pub struct Evento {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub league: Liga,
    pub teams: Times,
    pub time_start: String,
    pub time_end: String,
    pub players: Vec<String>,
}

#[derive(Clone, Deserialize)]
pub struct Liga {
    pub name: String,
    pub image: String,
}

#[derive(Clone, Deserialize)]
pub struct Times {
    pub home: Time,
    pub away: Time,
}

#[derive(Clone, Deserialize)]
pub struct Time {
    pub name: String,
    pub image: String,
}

pub fn baixar() -> Option<Vec<Evento>> {
    let json = crate::rede::json("https://embedtv.cc/api/events")?;
    serde_json::from_value(json).ok()
}
