use serde::Deserialize;

// Espelha o JSON da embedtv: nem todo campo é usado na tela.
#[allow(dead_code)]
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

impl Evento {
    pub fn horario(&self) -> String {
        self.time_start.split('T').nth(1).and_then(|s| s.get(0..5)).unwrap_or("").to_string()
    }
}

// Espelha o JSON da embedtv: nem todo campo é usado na tela.
#[allow(dead_code)]
#[derive(Clone, Deserialize)]
pub struct Liga {
    pub name: String,
    pub image: String,
}

// Espelha o JSON da embedtv: nem todo campo é usado na tela.
#[allow(dead_code)]
#[derive(Clone, Deserialize)]
pub struct Times {
    pub home: Time,
    pub away: Time,
}

// Espelha o JSON da embedtv: nem todo campo é usado na tela.
#[allow(dead_code)]
#[derive(Clone, Deserialize)]
pub struct Time {
    pub name: String,
    pub image: String,
}

pub fn baixar() -> Option<Vec<Evento>> {
    let json = crate::rede::json("https://embedtv.cc/api/events")?;
    let mut eventos: Vec<Evento> = serde_json::from_value(json).ok()?;
    
    let now = chrono::Utc::now();
    eventos.retain(|e| {
        let mut fim = chrono::DateTime::parse_from_rfc3339(&e.time_end).ok();
        let inicio = chrono::DateTime::parse_from_rfc3339(&e.time_start).ok();
        
        if fim.is_none() {
            if let Some(start) = inicio {
                fim = Some(start + chrono::Duration::hours(2));
            }
        }
        
        if let Some(final_fim) = fim {
            final_fim.with_timezone(&chrono::Utc) > now
        } else {
            true
        }
    });
    
    Some(eventos)
}
