use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct UserData {
    pub id: String,
    pub city: String,
    pub addr: String,
    pub lat: String,
    pub lon: String,
}

#[derive(Debug, Clone, Default)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub location: Location,
    pub distance: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Location {
    pub lat: String,
    pub lon: String,
}

#[derive(Debug, Clone, Default)]
pub struct BusDeparture {
    pub stop_id: String,
    pub planned: String,
    pub delay: Option<i64>,
    pub direction: String,
    /// Name of bus
    pub name: String,
    pub destination: Station,
    pub curr_position: Option<Location>,
}