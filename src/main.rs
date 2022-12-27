pub mod structs;
pub mod api;
pub mod io;
pub mod config;
#[cfg(test)]
mod tests;

use chrono::{DateTime, Duration};
use structs::*;
use api::*;
use io::*;

use dptree::{case, deps};
use tokio::{
    task::JoinHandle,
};
use std::{error::Error, vec, path::Path, fs::File, io::Read, sync::{Mutex, Arc}, collections::HashMap};
use teloxide::{
    dispatching::{dialogue, dialogue::InMemStorage},
    dptree::endpoint,
    filter_command,
    payloads::SendMessageSetters,
    prelude::*,
    types::{
        InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, KeyboardRemove,
        ParseMode::{Html, MarkdownV2},
        ReplyMarkup, MessageCommon, MessageKind,
    },
    utils::command::BotCommands,
};

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type MyDialogue = Dialogue<State, InMemStorage<State>>;
type MyTasksMap = Arc<Mutex<HashMap<String, JoinHandle<HandlerResult>>>>;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "Display help menu showing the commands list")]
    Help,
    #[command(description = "Start tracking your transit.")]
    Start,
    #[command(description = "Cancel the tracking.")]
    Cancel,
}

#[derive(Clone, Default)]
enum State {
    #[default]
    Start,
    ReceiveCity,
    ReceiveAddress {
        city: String,
    },
    ReceiveStop {
        city: String,
        addr: String,
        stations: Vec<Station>,
    },
    ReceiveBus {
        city: String,
        addr: String,
        stations: Vec<Station>,
        stop: String,
        stop_id: String,
    },
    ReceiveCancel {
        city: String,
        addr: String,
        stations: Vec<Station>,
        stop: String,
        stop_id: String,
        bus: String
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    log::info!("Starting 'Where is my transit' BOT ...");

    let bot = Bot::from_env();

    let command_handler = filter_command::<Command, _>()
        .branch(
            case![State::Start]
                .branch(case![Command::Help].endpoint(help))
                .branch(case![Command::Start].endpoint(start)),
        )
        .branch(case![Command::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(case![State::ReceiveCity].endpoint(receive_city))
        .branch(case![State::ReceiveAddress { city }].endpoint(receive_address))
        .branch(endpoint(invalid_state));
    
    let callback_query_handler = Update::filter_callback_query()
        .branch(
            case![State::ReceiveStop {
                city,
                addr,
                stations
            }]
            .endpoint(receive_stop),
        )
        .branch(
            case![State::ReceiveBus {
                city,
                addr,
                stations,
                stop,
                stop_id,
            }]
            .endpoint(receive_bus),
        )
        .branch(
            case![State::ReceiveCancel {
                city,
                addr,
                stations,
                stop,
                stop_id,
                bus
            }]
            .endpoint(receive_cancel)
        );

    let dial = dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler);
    
    // Shared HashMap of JoinHandles of tasks to be able to cancel the timer.
    let tasks: Arc<Mutex<HashMap<String, JoinHandle<HandlerResult>>>> = Arc::new(Mutex::new(HashMap::new()));

    Dispatcher::builder(bot, dial)
        .dependencies(deps![InMemStorage::<State>::new(), tasks])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn invalid_state(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}

//////////////////////////////////////////////////////////
// State handlers
//////////////////////////////////////////////////////////
async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, "🚫 Cancelling the tracking!")
        .await?;
    dialogue.exit().await?;
    Ok(())
}

async fn start(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let user_id = match &msg.kind {
        MessageKind::Common(MessageCommon {
             from, ..
            }) => {Some(from.as_ref().unwrap().id)}
        _ => {None}
    };
    
    let UserId(id_num) = user_id.unwrap();
    match get_user_data(id_num.to_string()) {
        Ok(data) => {
            let stations = get_nearby_stations(data.lat.clone(), data.lon.clone()).await?;
            let mut stations_names = stations
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<&str>>();

            let mut kb_buttons: Vec<&str> = vec![];
            kb_buttons.append(&mut stations_names);
            kb_buttons.push("<< Change address");
            
            let kb = make_inline_keyboard(kb_buttons, 2);
            // Parse station names as keyboard buttons
            // let kb = make_inline_keyboard(stations_names.clone(), 2);
            bot.send_message(
                msg.chat.id,
                format!(
                    "I found your last used address: \n*{}, {} 📍*\n
Now please select which bus station you want to track 👀\\.\n
Here are the nearby bus stations:",
                    data.addr, data.city
                ),
            )
            .parse_mode(MarkdownV2)
            .reply_markup(kb)
            .await?;
            dialogue.update(State::ReceiveStop { city: data.city, addr: data.addr, stations }).await?
        }
        Err(_) => {
            bot.send_message(
                msg.chat.id,
                format!("🔰 Let's start tracking a transit 🚌🚇!\n\nWhat city do you live in?"),
            )
            .reply_markup(KeyboardRemove::new())
            .await?;
            // println!("{:#?}", msg);
            dialogue.update(State::ReceiveCity).await?;
        }
    }
    Ok(())
}

async fn receive_city(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    match msg.text().map(ToOwned::to_owned) {
        Some(city) => {
            bot.send_message(
                msg.chat.id,
                format!("Awesome so you live in *{}* 🏙\\!\n\nWhat is your locations *address* so i can search for nearby transit stops?", city),
            ).parse_mode(MarkdownV2)
            .await?;
            dialogue.update(State::ReceiveAddress { city }).await?;
        }
        None => {
            bot.send_message(
                msg.chat.id,
                "❌ Please, send me your city that you live in, so i can start tracking.",
            )
            .await?;
        }
    }
    Ok(())
}

async fn receive_address(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    city: String,
) -> HandlerResult {
    match msg.text().map(ToOwned::to_owned) {
        Some(addr) => {
            // Store info of user for next time
            let user_id = match &msg.kind {
                MessageKind::Common(MessageCommon {
                    from, ..
                    }) => {Some(from.as_ref().unwrap().id)}
                _ => {None}
            };
 
            let UserId(id_num) = user_id.unwrap();

            let geocode = fetch_geocode(addr.clone(), city.clone()).await?;
            let stations = get_nearby_stations(geocode.0.clone(), geocode.1.clone()).await?;
            let mut stations_names = stations
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<&str>>();

            let user_data = UserData {
                id: id_num.to_string(),
                city: city.clone(),
                addr: addr.clone(),
                lat: geocode.0,
                lon: geocode.1
            };
            store_user_data(user_data).unwrap();

            let mut kb_buttons: Vec<&str> = vec![];
            kb_buttons.append(&mut stations_names);
            kb_buttons.push("<< Change address");
            
            let kb = make_inline_keyboard(kb_buttons, 2);
            bot.send_message(
                msg.chat.id,
                format!(
                    "Thank you\\! So your address is: \n*{}, {} 📍*\n\n
Now please select which bus station you want to track 👀\\.\n
Here are the nearby bus stations:",
                    addr, city
                ),
            )
            .parse_mode(MarkdownV2)
            .reply_markup(kb)
            .await?;

            dialogue
                .update(State::ReceiveStop {
                    city,
                    addr,
                    stations,
                })
                .await?;
        }
        None => {
            bot.send_message(
                msg.chat.id,
                "❌ Please, send me your city that you live in, so i can start tracking.",
            )
            .await?;
        }
    }
    Ok(())
}

async fn receive_stop(
    bot: Bot,
    dialogue: MyDialogue,
    (city, addr, stations): (String, String, Vec<Station>),
    q: CallbackQuery,
) -> HandlerResult {
    if let Some(stop) = &q.data {
        let chat_id = q.message.as_ref().unwrap().chat.id;
        let message_id = q.message.as_ref().unwrap().id;
        let null_kb = InlineKeyboardMarkup::default();

        if stop.starts_with("<<") {
            bot.edit_message_text(chat_id, message_id, "Ok, Send me the new 'address'.")
                .reply_markup(null_kb.clone()).await?;
            dialogue.update(State::ReceiveAddress { city: city.clone() }).await?
        };

        // Remove buttons from last send msg
        bot.edit_message_reply_markup(chat_id, message_id)
            .reply_markup(null_kb)
            .await?;

        let stop_id = stations
            .iter()
            .find(|&x| x.name.starts_with(stop))
            .unwrap()
            .id
            .to_owned();
        let departures = get_departures(stop_id.clone()).await?;

        let departures_names = departures
            .iter()
            .map(|x| x.name.as_str())
            .collect::<Vec<&str>>();

        let kb = make_inline_keyboard(departures_names.clone(), 3);

        // Format departure info
        let mut departure_info = String::new();
        for dep in departures.iter() {
            let time = chrono::DateTime::parse_from_rfc3339(&dep.planned)
                .unwrap()
                .time()
                .format("%H:%M");
            departure_info = format!(
                "{}\n--------------------\n{}, <b>to</b> {} <b>on</b> {}\n--------------------",
                departure_info,
                dep.name,
                dep.direction,
                time
            );
        }

        // Output departure info
        bot.send_message(
            dialogue.chat_id(),
            format!(
                "🚏 Infos for selected station: <b>{}</b>\n{}",
                stop, departure_info
            ),
        ).parse_mode(Html)
        .await?;

        // Send buttons for the bus departures
        bot.send_message(dialogue.chat_id(), "Select a bus:")
            .parse_mode(MarkdownV2)
            .reply_markup(kb)
            .await?;

        dialogue
            .update(State::ReceiveBus {
                city,
                addr,
                stations,
                stop: stop.to_string(),
                stop_id,
            })
            .await?;
    }
    Ok(())
}


async fn receive_bus(
    bot: Bot,
    dialogue: MyDialogue,
    (city, addr, stations, stop, stop_id): (String, String, Vec<Station>, String, String),
    q: CallbackQuery,
    tasks: MyTasksMap,
) -> HandlerResult {
    if let Some(bus) = &q.data {
        let dial = dialogue.clone();
        let n = 1;
        let msg_clone = q.message.clone().unwrap();
        let mut msg = msg_clone.clone();
        let stop_clone = stop.clone();
        let bus_clone = bus.clone();
        let stop_id_clone= stop_id.clone();

        let mut time_now = msg.date.clone() - Duration::minutes(n);
        let mut interval_timer = tokio::time::interval(Duration::minutes(n).to_std().unwrap());

        let task: JoinHandle<HandlerResult> = tokio::spawn( async move {
            loop {
                interval_timer.tick().await;
                time_now += Duration::minutes(n);

                let deps = get_departures(stop_id.clone()).await?;
                let dep = deps.iter().find(|&x| &x.name == &bus_clone.to_owned()).unwrap();
                
                let mut dep_time = DateTime::parse_from_rfc3339(&dep.planned).unwrap();
                let dur = dep_time.signed_duration_since(time_now);

                if let Some(del) = dep.delay {
                    dep_time += Duration::seconds(del);
                }
                
                if time_now > dep_time {
                    break;
                }

                let kb = make_inline_keyboard(vec!["<< Cancel"], 1);

                bot.delete_message(dial.chat_id(), msg.id)
                    .await?;

                msg = bot.send_message(dial.chat_id(), 
                    format!("🔔 Your bus: <b>{}</b> 🚌 comes in <b>{}</b> minutes ⌛!", &bus_clone, dur.num_minutes())
                )
                .parse_mode(Html)
                .reply_markup(kb)
                .await?;
            }

            bot.send_message(
                dial.chat_id(),
                format!("🔔 Your bus: *{}* 🚌 already departured from *{}*\\!", bus_clone, stop),
            ).parse_mode(MarkdownV2)
            .await?;

            dial.exit().await?;
            return Ok(());
        });

        {
            let mut t = tasks.lock().unwrap();
            let user = get_user_id(msg_clone);
            t.insert(user, task);
        }

        dialogue.update(State::ReceiveCancel { city, addr, stations, stop: stop_clone, stop_id: stop_id_clone, bus: bus.to_string()}).await?;
    }
    Ok(())
}

async fn receive_cancel(
    bot: Bot,
    dialogue: MyDialogue,
    (_city, _addr, _stations, _stop, _stop_id, _bus): (String, String, Vec<Station>, String, String, String),
    q: CallbackQuery,
    tasks: MyTasksMap
) -> HandlerResult {
    if let Some(d) = &q.data {
        if d.starts_with("<<") {
            let user = get_user_id(q.message.unwrap());
            let t = tasks.lock().unwrap();
            let v = t.get(&user).unwrap();
            v.abort();
        }
        bot.send_message(dialogue.chat_id(), "🚫 Cancelled! You can start over using /start!").await?;
        dialogue.exit().await?;
    }
    Ok(())
}

fn get_user_id(msg: Message) -> String{
    let user_id = match &msg.kind {
        MessageKind::Common(MessageCommon {
             from, ..
            }) => {Some(from.as_ref().unwrap().id)}
        _ => {None}
    };
    
    let UserId(id_num) = user_id.unwrap();
    id_num.to_string()
}

//////////////////////////////////////////////////////////
// Keyboards
//////////////////////////////////////////////////////////
/// Creates a keyboard made by buttons in a big column.
fn make_inline_keyboard(list: Vec<&str>, chunks: usize) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];

    for values in list.chunks(chunks) {
        let row = values
            .iter()
            .map(|&version| InlineKeyboardButton::callback(version.to_owned(), version.to_owned()))
            .collect();

        keyboard.push(row);
    }

    InlineKeyboardMarkup::new(keyboard)
}

/// Creates a keyboard made by buttons in a big column.
fn _make_keyboard() -> KeyboardMarkup {
    let mut keyboard: Vec<Vec<KeyboardButton>> = vec![];

    let debian_versions = ["Zew", "Zew2", "Zew3", "/cancel"];

    for versions in debian_versions.chunks(3) {
        let row = versions
            .iter()
            .map(|&version| KeyboardButton::new(format!("{}", version)))
            .collect();

        keyboard.push(row);
    }
    KeyboardMarkup::new(keyboard)
}

/// Creates a keyboard made by buttons in a big column.
fn _make_reply_keyboard() -> ReplyMarkup {
    let mut keyboard: Vec<Vec<KeyboardButton>> = vec![];

    let debian_versions = ["Zew", "Zew2", "Zew3", "/cancel"];

    for versions in debian_versions.chunks(3) {
        let row = versions
            .iter()
            .map(|&version| KeyboardButton::new(format!("{}", version)))
            .collect();

        keyboard.push(row);
    }
    ReplyMarkup::keyboard(keyboard)
}


