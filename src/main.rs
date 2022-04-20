use dotenv::dotenv;
use if_chain::if_chain;
use rand::Rng;
use strum::{Display, EnumIter, EnumString, IntoEnumIterator};
use teloxide::adaptors::DefaultParseMode;
use teloxide::dispatching::dialogue::{
    serializer::Json, ErasedStorage, GetChatId, SqliteStorage, Storage,
};
use teloxide::dispatching::{MessageFilterExt, UpdateFilterExt};
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::*;
use teloxide::requests::RequesterExt;
use teloxide::types::{Chat, InlineKeyboardButton, InlineKeyboardMarkup, Me, ParseMode};
use teloxide::utils::command::BotCommands;

use std::str::FromStr;

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

type MyBot = AutoSend<DefaultParseMode<Bot>>;
type MyDialogue = Dialogue<State, ErasedStorage<State>>;
type MyStorage = std::sync::Arc<ErasedStorage<State>>;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum State {
    Start,
    Language(LastFlag),
    ReceiveName(Language, i32),
    ConfirmName {
        lang: Language,
        name: String,
        msg_id: i32,
    },
    ReceiveMode {
        user: User,
        last: LastFlag,
    },
    SelectedMode {
        user: User,
    },
}

impl Default for State {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct LastFlag(bool);

#[derive(Clone, serde::Serialize, serde::Deserialize, EnumIter, EnumString, Display, Debug)]
pub enum Language {
    EN,
    UA,
}

impl Language {
    fn name(&self) -> &'static str {
        match self {
            Language::EN => "EN 🇬🇧",
            Language::UA => "UA 🇺🇦",
        }
    }

    fn callback(&self) -> String {
        self.to_string()
    }

    fn from_callback(s: &str) -> Self {
        Self::from_str(s).expect("callback to be valid")
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct User {
    name: String,
    lang: Language,
    mode: Option<Mode>,
}

impl User {
    fn new(lang: Language, name: String) -> Self {
        Self {
            lang,
            name,
            mode: None,
        }
    }

    fn describe(&self) -> String {
        match self.lang {
            Language::EN => format!("{} ⫶ unselected", self.name),
            Language::UA => format!("{} ⫶ не вибрано", self.name),
        }
    }
}

#[derive(Clone, EnumIter, EnumString, Display, serde::Serialize, serde::Deserialize, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum Mode {
    Obimy,
    Spotify,
    #[strum(serialize = "dnd")]
    Dungeons,
}

impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::Obimy => "Obimy",
            Mode::Spotify => "D&D",
            Mode::Dungeons => "Spotify",
        }
    }

    fn callback(&self) -> String {
        self.to_string()
    }

    fn from_callback(s: &str) -> Self {
        Self::from_str(s).expect("callback to be valid")
    }
}

#[derive(EnumIter, EnumString, Display, Clone)]
#[strum(serialize_all = "snake_case")]
pub enum NameAction {
    Ok,
}

impl NameAction {
    fn name(&self) -> &'static str {
        "Ok"
    }

    fn callback(&self) -> String {
        self.to_string()
    }

    fn from_callback(s: &str) -> Self {
        Self::from_str(s).expect("callback to be valid")
    }
}

// type ModeDialogue = Dialogue<ModeState, ErasedStorage<ModeState>>;
// type ModeStorage = std::sync::Arc<ErasedStorage<ModeState>>;

#[derive(BotCommands, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "display this message.")]
    Help,
    #[command(description = "select mode.")]
    Mode,
    #[command(description = "handle a username.")]
    Username(String),
    #[command(description = "handle a username and an age.", parse_with = "split")]
    UsernameAndAge { username: String, age: u8 },
}

#[derive(BotCommands, Clone)]
#[command(rename = "lowercase", description = "Callback commands")]
enum CallbackCommand {
    #[command(description = "resend keyboard message")]
    Resend,
}

#[derive(BotCommands, Clone)]
#[command(rename = "lowercase", description = "Maintainer commands")]
enum MaintainerCommand {
    #[command(parse_with = "split", description = "generate a number within range")]
    Rand {
        from: u64,
        to: u64,
    },
    Reset,
}

#[derive(Clone)]
struct ConfigParameters {
    bot_maintainer: u64,
    maintainer_username: Option<String>,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();
    log::info!("Starting dices bot");

    let bot = Bot::from_env().parse_mode(ParseMode::Html).auto_send();

    let parameters = ConfigParameters {
        bot_maintainer: 364448153,
        maintainer_username: None,
    };

    let storage: MyStorage = SqliteStorage::open("db.sqlite", Json)
        .await
        .unwrap()
        .erase();

    // let mode_storage: ModeStorage = SqliteStorage::open("db.sqlite", Json)
    //     .await
    //     .unwrap()
    //     .erase();

    let handler = dptree::entry()
        // callbacks
        .branch(
            Update::filter_callback_query()
                .enter_dialogue::<CallbackQuery, ErasedStorage<State>, State>()
                .branch(teloxide::handler![State::Language(last)].endpoint(language_callback))
                .branch(
                    teloxide::handler![State::ReceiveMode { user, last }].endpoint(mode_callback),
                )
                .branch(
                    teloxide::handler![State::ConfirmName { lang, name, msg_id }]
                        .endpoint(confirm_name_callback),
                ),
        )
        .branch(
            Update::filter_message()
                // commands
                .branch(
                    dptree::entry()
                        .filter_command::<Command>()
                        .endpoint(comands),
                )
                // maintainer commands
                .branch(maintainer_commands())
                // text dialogue
                .branch(
                    Message::filter_text()
                        .enter_dialogue::<Message, ErasedStorage<State>, State>()
                        .branch(teloxide::handler![State::Start].endpoint(start))
                        .branch(
                            teloxide::handler![State::Language(last)].endpoint(language_message),
                        )
                        .branch(
                            teloxide::handler![State::ReceiveName(lang, msg_id)]
                                .endpoint(name_message),
                        )
                        .branch(
                            teloxide::handler![State::ConfirmName { lang, name, msg_id }]
                                .endpoint(confirm_name_message),
                        )
                        .branch(
                            teloxide::handler![State::ReceiveMode { user, last }]
                                .endpoint(mode_message),
                        )
                        .branch(
                            teloxide::handler![State::SelectedMode { user }].endpoint(mode_message),
                        ),
                ),
        );

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![storage, parameters])
        .default_handler(|upd| async move {
            log::warn!("Unhandled update: {:?}", upd);
        })
        .error_handler(LoggingErrorHandler::with_custom_text(
            "An error has occurred in the dispatcher",
        ))
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;
}

// TODO: try to refactor
fn maintainer_commands() -> dptree::Handler<
    'static,
    DependencyMap,
    HandlerResult,
    teloxide::dispatching::DpHandlerDescription,
> {
    dptree::filter(|msg: Message, cfg: ConfigParameters| {
        msg.from()
            .map(|user| user.id.0 == cfg.bot_maintainer)
            .unwrap_or_default()
    })
    .filter_command::<MaintainerCommand>()
    .endpoint(
        |bot: MyBot, msg: Message, cmd: MaintainerCommand, storage: MyStorage| async move {
            match cmd {
                MaintainerCommand::Rand { from, to } => {
                    let mut rng = rand::rngs::OsRng::default();
                    let value = rng.gen_range(from..=to);
                    bot.send_message(
                        msg.chat.id,
                        format!("Hello maintainer! Your rand value: {}", &value),
                    )
                    .await?;
                }
                MaintainerCommand::Reset => {
                    MyDialogue::new(storage, msg.chat.id).reset().await?;
                }
            }
            Ok(())
        },
    )
}

async fn start(bot: MyBot, msg: Message, dialogue: MyDialogue, me: Me) -> HandlerResult {
    let bot_name = me.user.username.unwrap();
    log::info!("bot name: {}", bot_name);
    send_languages(&bot, &msg).await?;
    dialogue.update(State::Language(LastFlag(true))).await?;
    Ok(())
}

async fn send_languages(bot: &MyBot, msg: &Message) -> HandlerResult {
    let keyboard = languages_keyboard();
    bot.send_message(msg.chat.id, format!("Let's start! What's your language?"))
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn name_message(
    bot: MyBot,
    msg: Message,
    dialogue: MyDialogue,
    (lang, msg_id): (Language, i32),
) -> HandlerResult {
    match msg.text() {
        Some(name) => {
            send_confirm_name(&bot, &msg.chat, msg_id, &lang, name, None).await?;
            dialogue
                .update(State::ConfirmName {
                    lang,
                    name: name.to_string(),
                    msg_id,
                })
                .await?;
        }
        None => {
            bot.send_message(msg.chat.id, "Send me plain text.").await?;
        }
    }

    Ok(())
}

async fn send_confirm_name(
    bot: &MyBot,
    chat: &Chat,
    msg_id: i32,
    lang: &Language,
    name: &str,
    old_name: Option<&str>,
) -> HandlerResult {
    let keyboard = name_keyboard();
    let greet = match lang {
        Language::EN => "Hello",
        Language::UA => "Привіт",
    };
    let text = match old_name {
        Some(old_name) => format!("{}, <s>{}</s>{}", greet, old_name, name),
        None => format!("{}, {}", greet, name),
    };
    bot.edit_message_text(chat.id, msg_id, text)
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn send_modes(bot: &MyBot, user: &User, msg: &Message, msg_id: Option<i32>) -> HandlerResult {
    let text = user.describe();
    if let Some(msg_id) = msg_id {
        bot.edit_message_text(msg.chat.id, msg_id, text)
            .reply_markup(modes_keyboard())
            .await?;
    } else {
        bot.send_message(msg.chat.id, text)
            .reply_markup(modes_keyboard())
            .await?;
    }
    Ok(())
}

async fn language_message(bot: MyBot, msg: Message, dialogue: MyDialogue, me: Me) -> HandlerResult {
    let ans = msg.text().unwrap();
    let bot_name = me.user.username.unwrap();
    match CallbackCommand::parse(ans, bot_name) {
        Ok(CallbackCommand::Resend) => {
            // bot.send_message(msg.chat.id, "Resending languages keyboard")
            //     .await?;
            send_languages(&bot, &msg).await?;
        }
        _ => {
            bot.send_message(msg.chat.id, "Please, select your language. /resend")
                .await?;
        }
    }
    dialogue.update(State::Language(LastFlag(false))).await?;

    Ok(())
}

async fn language_callback(
    bot: MyBot,
    q: CallbackQuery,
    dialogue: MyDialogue,
    state: State,
) -> HandlerResult {
    let chat_id = q.chat_id();
    if_chain! {
        if let State::Language(last) = state;
        if let Some(lang) = q.data;
        then {
            let lang = Language::from_callback(lang.as_str());
            log::info!("selected language {:?} for user {:?}", lang, chat_id);

            let text = match lang {
                Language::EN => "What's your name?",
                Language::UA => "Як тебе звати?",
            };
            match q.message {
                Some(Message { id, chat, .. }) => {
                  let msg_id = if last.0 {
                        bot.edit_message_text(chat.id, id, text).await?;
                        id
                    } else {
                        bot.delete_message(chat.id, id).await?;
                        let sent = bot.send_message(chat.id, text).await?;
                        sent.id
                    };
                dialogue.update(State::ReceiveName(lang, msg_id)).await?;
                }
                None => {
                    if let Some(id) = q.inline_message_id {
                        bot.edit_message_text_inline(id, text).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn confirm_name_message(
    bot: MyBot,
    msg: Message,
    state: State,
    dialogue: MyDialogue,
) -> HandlerResult {
    if_chain! {
        if let State::ConfirmName{lang, name: old_name, msg_id } = state;
        if let Some(new_name) = msg.text();
        then{
            send_confirm_name(&bot, &msg.chat, msg_id, &lang, new_name, Some(&old_name)).await?;
            dialogue.update(State::ConfirmName{lang, name: new_name.to_string(), msg_id}).await?;
        }
    }

    Ok(())
}

async fn confirm_name_callback(
    bot: MyBot,
    q: CallbackQuery,
    state: State,
    dialogue: MyDialogue,
) -> HandlerResult {
    if_chain! {
    if let State::ConfirmName { lang, name, msg_id } = state;
    if let Some(msg) = q.message;
    then{
        let user = User::new(lang, name.to_string());
        send_modes(&bot, &user, &msg, Some(msg_id)).await?;
        dialogue
            .update(State::ReceiveMode {
                user,
                last: LastFlag(true),
            })
            .await?;
    }}

    Ok(())
}

async fn mode_message(
    bot: MyBot,
    msg: Message,
    me: Me,
    state: State,
    dialogue: MyDialogue,
) -> HandlerResult {
    if let State::ReceiveMode { user, .. } = state {
        let ans = msg.text().unwrap();
        let bot_name = me.user.username.unwrap();
        match CallbackCommand::parse(ans, bot_name) {
            Ok(CallbackCommand::Resend) => {
                // bot.send_message(msg.chat.id, "Resending modes keyboard")
                //     .await?;
                send_modes(&bot, &user, &msg, None).await?;
            }
            _ => {
                let text = match user.lang {
                    Language::EN => "Please, select the mode",
                    Language::UA => "Будь ласка, оберіть режим",
                };
                bot.send_message(msg.chat.id, format!("{}. /resend", text))
                    .await?;
            }
        }
        dialogue
            .update(State::ReceiveMode {
                user,
                last: LastFlag(false),
            })
            .await?;
    }

    Ok(())
}

async fn mode_callback(
    bot: MyBot,
    q: CallbackQuery,
    dialogue: MyDialogue,
    state: State,
) -> HandlerResult {
    if_chain! {
        if let State::ReceiveMode{mut user, last} = state;
        if let Some(data) = q.data;
        if let Some(Message {id, chat, ..}) = q.message;
        then {
            let mode = Mode::from_callback(&data);
            user.mode = Some(mode);
            let text = format!("chat_id: {}; user: {:?}; mode: {:?}", chat.id, user, user.mode);
            log::info!("{}",text);
            dialogue.update(State::SelectedMode{user}).await?;
            if last.0 {
                bot.edit_message_text(chat.id, id, text).await?;
            }else{
                bot.delete_message(chat.id, id).await?;
                bot.send_message(chat.id, text).await?;
            }
        }
    }

    Ok(())
}

async fn receive_location(
    bot: MyBot,
    msg: Message,
    dialogue: MyDialogue,
    (full_name, age): (String, u8),
) -> HandlerResult {
    match msg.text() {
        Some(location) => {
            let message = format!(
                "Full name: {}\nAge: {}\nLocation: {}",
                full_name, age, location
            );
            bot.send_message(msg.chat.id, message).await?;
            dialogue.exit().await?;
        }
        None => {
            bot.send_message(msg.chat.id, "Send me plain text.").await?;
        }
    }

    Ok(())
}

async fn comands(bot: MyBot, msg: Message, cmd: Command, cfg: ConfigParameters) -> HandlerResult {
    match cmd {
        Command::Help => {
            log::info!("help command for {}", msg.chat.id);
            let text = if msg.from().unwrap().id.0 == cfg.bot_maintainer {
                format!(
                    "{}\n{}",
                    Command::descriptions(),
                    MaintainerCommand::descriptions()
                )
            } else {
                Command::descriptions().to_string()
            };
            bot.send_message(msg.chat.id, text).await?;
        }
        Command::Mode => {
            let keyboard = modes_keyboard();
            bot.send_message(msg.chat.id, "Select mode:")
                .reply_markup(keyboard)
                .await?;
        }
        Command::Username(username) => {
            let text = format!("Your name is @{}", username);
            bot.send_message(msg.chat.id, text).await?;
        }
        Command::UsernameAndAge { username, age } => {
            let text = format!("Your username is @{} and age is {}", username, age);
            bot.send_message(msg.chat.id, text).await?;
        }
    };

    Ok(())
}

fn languages_keyboard() -> InlineKeyboardMarkup {
    let mut keyboard = vec![];
    let row: Vec<_> = Language::iter()
        .map(|lang| {
            InlineKeyboardButton::callback(lang.name().to_owned(), lang.callback().to_owned())
        })
        .collect();
    keyboard.push(row);
    InlineKeyboardMarkup::new(keyboard)
}

fn modes_keyboard() -> InlineKeyboardMarkup {
    let keyboard: Vec<_> = Mode::iter()
        .map(|mode| {
            vec![InlineKeyboardButton::callback(
                mode.name().to_owned(),
                mode.callback().to_owned(),
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(keyboard)
}

fn name_keyboard() -> InlineKeyboardMarkup {
    let ok = NameAction::Ok;
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        ok.name().to_owned(),
        ok.callback().to_owned(),
    )]])
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        let word = "hello";
        let count = word.chars().count();
        assert_eq!(count, 5);
    }
}
