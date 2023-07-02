use std::fs;

use crate::*;

//////////////////////////////////////////////////////////
// I/O
//////////////////////////////////////////////////////////
pub fn store_user_data(user: UserData) -> Result<(), Box<dyn Error + Sync + Send>> {
    let user_data_path = std::env::var("USER_DATA_PATH").expect("USER_DATA_PATH must be set.");
    let path = Path::new(&user_data_path);

    let file_data = match fs::read_to_string(path) {
        Ok(f) => f,
        Err(_) => "".to_string()
    };

    let mut users: Vec<UserData> = match serde_json::from_str(&file_data) {
        Ok(u) => u,
        Err(_) => vec![]
    };

    match users.iter_mut().find(|x| x.id == user.id) {
        Some(u) => {
            *u = user;
        },
        None => {
            users.push(user)
        }
    }

    let json = serde_json::to_string(&users).unwrap();
    fs::write(path, json.as_bytes()).unwrap();

    Ok(())
}

pub fn get_user_data(user_id: String) -> Result<UserData, Box<dyn Error + Sync + Send>> {
    let user_data_path = std::env::var("USER_DATA_PATH").expect("USER_DATA_PATH must be set.");
    let path = Path::new(&user_data_path);

    let mut file_data = String::new();
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {return Err(e)?}
    };
    file.read_to_string(&mut file_data).expect("Unable to read to string");
    
    let users: Vec<UserData> = serde_json::from_str(&file_data)?;

    match users.iter().find(|&u| u.id == user_id) {
        Some(user) => Ok(user.to_owned()),
        None => Err("User not found!")?
    }
}