use crate::structs::*;

use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use std::{
    error::Error
};

//////////////////////////////////////////////////////////
// API calls
//////////////////////////////////////////////////////////
pub async fn fetch_geocode(
    addr: String,
    city: String,
) -> Result<(String, String), Box<dyn Error + Send + Sync>> {
    let url =
        format!("https://nominatim.openstreetmap.org/search?street={addr}&city={city}&format=json");

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "reqwest/0.11.13")
        .send()
        .await?
        .text()
        .await?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
        let lat = match json[0].get("lat") {
            Some(v) => v,
            None => &serde_json::Value::Null,
        };
        let lon = match json[0].get("lon") {
            Some(v) => v,
            None => &serde_json::Value::Null,
        };
        let lat = match lat.as_str() {
            Some(n) => n.to_owned(),
            None => "0".to_owned(),
        };
        let lon = match lon.as_str() {
            Some(n) => n.to_owned(),
            None => "0".to_owned(),
        };
        return Ok((lat, lon));
    }
    Err("Fetching geocode!")?
}

pub async fn get_nearby_stations(
    lat: String,
    lon: String,
) -> Result<Vec<Station>, Box<dyn Error + Send + Sync>> {
    let url = format!("https://v5.db.transport.rest/stops/nearby?latitude={lat}&longitude={lon}");

    let mut stations_value: Vec<serde_json::Value> = vec![];

    let response = reqwest::get(url).await?.text().await?;
    let json: serde_json::Value = serde_json::from_str(&response).unwrap();

    // Change using iterators
    let mut i = 0;
    while json[i].is_object() {
        stations_value.push(json[i].to_owned());
        i += 1;
    }

    let mut stations: Vec<Station> = vec![];
    for s in stations_value.iter() {
        let station_name = s.get("name").unwrap().as_str().unwrap().to_string();
        let station_name = station_name.splitn(2, ",").next().unwrap().to_string();
        let station = Station {
            id: s.get("id").unwrap().as_str().unwrap().to_string(),
            name: station_name,
            location: Location {
                lat: s
                    .get("location")
                    .unwrap()
                    .get("latitude")
                    .unwrap()
                    .to_string(),
                lon: s
                    .get("location")
                    .unwrap()
                    .get("longitude")
                    .unwrap()
                    .to_string(),
            },
            distance: s.get("distance").unwrap().as_i64().unwrap(),
        };
        stations.push(station);
    }

    Ok(stations)
}

pub async fn get_departures(
    stop_id: String,
) -> Result<Vec<BusDeparture>, Box<dyn Error + Send + Sync>> {
    let url = format!(
        "https://v5.db.transport.rest/stops/{}/departures",
        // "https://v5.db.transport.rest/stops/{}/departures?when=today 9am", // for debugging
        stop_id
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "reqwest/0.11.13")
        .send()
        .await?
        .text()
        .await?;

    let mut departures_values: Vec<serde_json::Value> = vec![];

    let json: serde_json::Value = serde_json::from_str(&resp).unwrap();

    // Change using iterators
    let mut i = 0;
    while json[i].is_object() {
        departures_values.push(json[i].to_owned());
        i += 1;
    }

    let mut buses: Vec<BusDeparture> = vec![];
    for d in departures_values.iter() {
        let bus_name = d
            .get("line")
            .unwrap()
            .get("name")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let bus_planned = d.get("plannedWhen").unwrap().as_str().unwrap().to_string();
        let bus_delay = match d.get("delay") {
            Some(i) => i.as_i64(),
            None => None,
        };
        let bus_direction = d.get("direction").unwrap().as_str().unwrap().to_string();

        let bus_destination = Station {
            id: d
                .get("destination")
                .unwrap()
                .get("id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            name: d
                .get("destination")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            location: Location {
                lat: d
                    .get("destination")
                    .unwrap()
                    .get("location")
                    .unwrap()
                    .get("latitude")
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_string(),
                lon: d
                    .get("destination")
                    .unwrap()
                    .get("location")
                    .unwrap()
                    .get("longitude")
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_string(),
            },
            distance: -1, // -1 means undefined
        };

        let bus_curr_position = match d.get("currentTripPosition") {
            Some(v) => Some(Location {
                lat: v.get("latitude").unwrap().as_f64().unwrap().to_string(),
                lon: v.get("longitude").unwrap().as_f64().unwrap().to_string(),
            }),
            None => None,
        };

        let bus = BusDeparture {
            stop_id: stop_id.clone(),
            planned: bus_planned,
            delay: bus_delay,
            direction: bus_direction,
            name: bus_name,
            destination: bus_destination,
            curr_position: bus_curr_position,
        };
        buses.push(bus);
    }
    Ok(buses)
}