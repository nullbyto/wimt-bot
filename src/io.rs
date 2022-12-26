use crate::config::*;
use crate::*;

//////////////////////////////////////////////////////////
// I/O
//////////////////////////////////////////////////////////
pub fn store_user_data(user: UserData) -> Result<(), Box<dyn Error + Sync + Send>> {
    let path = Path::new(USER_DATA_PATH);

    let mut file_data = String::new();
    let mut file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path)
        .expect("Unable to open file");
    file.read_to_string(&mut file_data).expect("Unable to read to string");

    let mut users: Vec<UserData> = match serde_json::from_str(&file_data) {
        Ok(u) => u,
        Err(_) => vec![]
    };

    // Check if user already exists
    match get_user_data(user.id.clone()) {
        // Replace existing user
        Ok(_) => {
            let u = users.iter_mut().find(|x| x.id == user.id).unwrap();
            *u = user;
        },
        // Append new user
        Err(_) => {users.push(user)}
    }

    println!("{:?}", users);
    serde_json::to_writer(file, &users)?;

    Ok(())
}

pub fn get_user_data(user_id: String) -> Result<UserData, Box<dyn Error + Sync + Send>> {
    let path = Path::new(USER_DATA_PATH);
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