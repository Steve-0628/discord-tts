#![warn(clippy::pedantic)]

mod commands;
mod config;
mod db;
mod filter;
mod songbird_handler;
mod voicevox;
mod wavsource;

use std::io::Cursor;

use reqwest::Url;
use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    model::{
        application::{command::Command, interaction::Interaction},
        channel::Message,
        gateway::Ready,
        prelude::GatewayIntents,
    },
};
use songbird::{tracks::create_player, SerenityInit};
use tap::Tap;

use crate::config::CONFIG;
use crate::db::PERSISTENT_DB;

struct Bot {
    voicevox: voicevox::Client,
    prefix: String,
}

#[async_trait]
impl EventHandler for Bot {
    async fn ready(&self, ctx: Context, ready: Ready) {
        Command::set_global_application_commands(&ctx.http, |commands| {
            commands
                .create_application_command(|cmd| commands::join::register(&self.prefix, cmd))
                .create_application_command(|cmd| commands::leave::register(&self.prefix, cmd))
                .create_application_command(|cmd| commands::skip::register(&self.prefix, cmd))
                .create_application_command(|cmd| commands::speaker::register(&self.prefix, cmd))
        })
        .await
        .unwrap();

        println!("{} is connected!", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) {
        let Some(content) = filter::filter(&ctx, &msg).await else {
            return;
        };

        let speaker = PERSISTENT_DB.get_speaker_id(msg.author.id);
        let mut wav = Cursor::new(self.voicevox.tts(&content, speaker).await);
        let (audio, _handle) = create_player(wavsource::wav_reader(&mut wav));

        let manager = songbird::get(&ctx)
            .await
            .expect("Songbird is not initialized");

        let handler = manager.get(msg.guild_id.unwrap()).unwrap();
        handler.lock().await.enqueue(audio);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let prefix = &self.prefix;
        match interaction {
            Interaction::ApplicationCommand(command) => match command.data.name.as_str() {
                s if s == format!("{prefix}speaker") => {
                    commands::speaker::run(&ctx, command, &self.voicevox).await;
                }
                s if s == format!("{prefix}join") => commands::join::run(&ctx, command).await,
                s if s == format!("{prefix}leave") => commands::leave::run(&ctx, command).await,
                s if s == format!("{prefix}skip") => commands::skip::run(&ctx, command).await,
                _ => unreachable!("Unknown command: {}", command.data.name),
            },
            Interaction::MessageComponent(interaction) => {
                commands::speaker::update(&ctx, interaction, &self.voicevox).await;
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let default_header = reqwest::header::HeaderMap::new().tap_mut(|h| {
        let Some(s) = &CONFIG.additional_headers else {
            return;
        };

        for s in s.split(',') {
            let mut split = s.split(':');

            let key = split.next().unwrap().trim();
            let value = split.next().unwrap().trim();

            h.insert(key, reqwest::header::HeaderValue::from_str(value).unwrap());
        }
    });

    let mut client = Client::builder(&CONFIG.discord_token, intents)
        .event_handler(Bot {
            voicevox: voicevox::Client::new(
                Url::parse(&CONFIG.voicevox_host).unwrap(),
                reqwest::Client::builder()
                    .default_headers(default_header)
                    .build()
                    .unwrap(),
            )
            .await,
            prefix: CONFIG.command_prefix.clone().unwrap_or(String::new()),
        })
        .register_songbird()
        .await
        .expect("Failed to create client");

    tokio::spawn(async move {
        let _: Result<_, _> = client
            .start()
            .await
            .map_err(|why| println!("Client ended: {why:?}"));
    });

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to wait Ctrl+C");

    println!("Received Ctrl+C, shutting down.");
}
