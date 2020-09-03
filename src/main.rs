#[macro_use]
extern crate log;

use std::env;
use std::result;
use serenity::Client;
use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::framework::StandardFramework;
use serenity::framework::standard::*;
use serenity::framework::standard::macros::*;
use regex::Regex;
use once_cell::sync::Lazy;

struct BobHandler;

#[group]
#[commands(build)]
struct Bob;


/// Initialize and start the bot.
fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");
    env::var("BOB_CHANNEL_NAME").expect("Missing BOB_CHANNEL_NAME");

    pretty_env_logger::init();
    debug!("Logger initialized!");

    let mut client = Client::new(&token, BobHandler).expect("Error creating Discord client");
    debug!("Discord client created!");

    client.with_framework(
        StandardFramework::new().configure(
            |c| c
                .prefix("!")
        )
        .group(&BOB_GROUP)
        .on_dispatch_error(error)
    );
    debug!("Client framework initialized!");

    client.start_autosharded().expect("Error starting Discord client");
    debug!("Autosharded Discord client started!");
}


impl EventHandler for BobHandler {

    /// Handle the ready event.
    fn ready(&self, _context: Context, ready: Ready) {
        info!("{} is ready!", &ready.user.name);
    }

    /// Called when the voice state of an user changes.
    // IntelliJ Rust inspection is broken
    // https://github.com/intellij-rust/intellij-rust/issues/1191
    // noinspection RsTraitImplementation
    fn voice_state_update(&self, ctx: Context, guild_id: Option<GuildId>, old: Option<VoiceState>, new: VoiceState) {
        debug!("Received a voice state update");

        match delete_left_temp_channel(ctx, guild_id, old, new) {
            Err(s) => {
                debug!("Not deleting: {}", s);
            }
            _ => (),
        }
    }

}

/// Handle command errors.
fn error(ctx: &mut Context, msg: &Message, error: DispatchError) {
    match error {
        DispatchError::OnlyForGuilds => {
            debug!("Rejecting command sent outside of a guild");
            let _ = msg.channel_id.say(&ctx.http, "⚠️ This command only works in a guild.");
        }
        DispatchError::CheckFailed(check, reason) => {
            match reason {
                Reason::Log(l) => {
                    error!("Check {} failed: {}", &check, &l);
                },
                Reason::User(u) => {
                    debug!("Check {} failed", &check);
                    let _ = msg.channel_id.say(&ctx.http, format!("⚠️ {}", &u));
                },
                Reason::UserAndLog {user: u, log: l} => {
                    error!("Check {} failed: {}", &check, &l);
                    let _ = msg.channel_id.say(&ctx.http, format!("⚠️ {}", &u));
                }
                _ => {
                    error!("Check {} failed for an unknown reason.", &check);
                }
            }
        }
        _ => {
            warn!("Unmatched error occoured!");
            let _ = msg.channel_id.say(&ctx.http, "☢️ An unhandled error just occoured! It has been logged to the console.");
        }
    }
}

/// Convert a string to **kebab-case**.
fn sanitize_channel_name(s: &str) -> String {
    static REPLACE_PATTERN: Lazy<Regex> = Lazy::new(|| {Regex::new("[^a-z0-9]").unwrap()});

    let mut last = s.len();
    if last > 100 {
        last = 100;
    }

    let s = &s[..last];
    let s = s.to_ascii_lowercase();
    let s: String = (*REPLACE_PATTERN).replace_all(&s, " ").into_owned();
    let s = s.trim();
    let s = s.replace(" ", "-");

    debug!("Sanitized channel name to: {}", &s);
    s
}

#[check]
#[name = "MatchChannelName"]
fn check_match_channel_name(ctx: &mut Context, msg: &Message, _args: &mut Args) -> CheckResult {
    static BOB_CHANNEL_NAME: Lazy<String> = Lazy::new(|| {env::var("BOB_CHANNEL_NAME").expect("Missing BOB_CHANNEL_NAME envvar.")});

    let channel = msg.channel(&ctx.cache);
    if channel.is_none() {
        return CheckResult::new_log("Could not fetch bot channel info from the Discord API.");
    }

    let channel = channel.unwrap();
    let channel = channel.guild();
    if channel.is_none() {
        return CheckResult::new_user("This channel isn't inside a server.");
    }

    let channel = channel.unwrap();
    let channel = channel.read();
    if channel.name != *BOB_CHANNEL_NAME {
        return CheckResult::new_user(format!("This channel isn't named #{}, so commands won't run here.", &*BOB_CHANNEL_NAME));
    }

    CheckResult::Success
}


#[check]
#[name = "EnsureCategory"]
fn check_ensure_category(ctx: &mut Context, msg: &Message, _args: &mut Args) -> CheckResult {
    let channel = msg.channel(&ctx.cache);
    if channel.is_none() {
        return CheckResult::new_log("Could not fetch bot channel info from the Discord API.");
    }

    let channel = channel.unwrap();
    let channel = channel.guild();
    if channel.is_none() {
        return CheckResult::new_user("This channel isn't inside a server.");
    }

    let channel = channel.unwrap();
    let channel = channel.read();
    if channel.category_id.is_none() {
        return CheckResult::new_user("This channel isn't inside a category.");
    }

    let category = channel.category_id.unwrap().to_channel(&ctx.http);
    if category.is_err() {
        return CheckResult::new_log("Could not fetch bot category info from the Discord API");
    }

    CheckResult::Success
}


#[check]
#[name = "EnsureConnectedToVoice"]
fn check_ensure_connected_to_voice(ctx: &mut Context, msg: &Message, _args: &mut Args) -> CheckResult {
    let guild = msg.guild(&ctx.cache);
    if guild.is_none() {
        return CheckResult::new_log("Could not fetch guild info from the Discord API.");
    }

    let guild = guild.unwrap();
    let guild = guild.read();

    let author_voice_state = guild.voice_states.get(&msg.author.id);
    if author_voice_state.is_none() {
        return CheckResult::new_user("You must be connected to a voice channel in order to run this command.");
    }

    CheckResult::Success
}


/// Build a new temporary channel.
#[command]
#[only_in(guilds)]
#[checks(MatchChannelName)]
#[checks(EnsureCategory)]
#[checks(EnsureConnectedToVoice)]
fn build(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    debug!("Running command: !build");

    let guild = msg.guild(&ctx.cache).unwrap();
    let guild = guild.read();
    let channel = msg.channel(&ctx.cache).unwrap().guild().unwrap();
    let channel = channel.read();
    let category = channel.category_id.unwrap().to_channel(&ctx.http).unwrap().category().unwrap();
    let category = category.read();

    let new_channel_name = sanitize_channel_name(args.rest());

    channel.broadcast_typing(&ctx.http)?;
    debug!("Started typing");

    let created = guild.create_channel(&ctx.http, |c| {
        debug!("Temp channel name will be: {}", &new_channel_name);
        c.name(new_channel_name);

        debug!("Temp channel type will be: Voice");
        c.kind(ChannelType::Voice);

        debug!("Temp channel category will be: {}", &channel.category_id.unwrap());
        c.category(&channel.category_id.unwrap());

        debug!("Temp channel permissions will be cloned from the category");
        let mut permissions = category.permission_overwrites.clone();
        permissions.push(PermissionOverwrite{
            allow: Permissions::all(),
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(msg.author.id.clone())
        });
        c.permissions(permissions)
    })?;
    info!("Created temp channel #{}", &created.name);

    msg.channel_id.say(&ctx.http, format!("🔨 Temp channel <#{}> was built.", &created.id))?;
    debug!("Sent channel created message");

    guild.move_member(&ctx.http, &msg.author.id, &created.id)?;
    debug!("Moved command caller to the created channel");

    Ok(())
}

/// Check whether an user left a channel and delete temp channels.
fn delete_left_temp_channel(ctx: Context, guild: Option<GuildId>, old: Option<VoiceState>, new: VoiceState) -> result::Result<(), &'static str> {
    let guild = guild.ok_or("Unknown guild_id")?;
    let guild: PartialGuild = guild.to_partial_guild(&ctx.http).or(Err("Could not fetch guild data"))?;

    let old = old.ok_or("User just joined voice chat")?;
    let old_channel = &old.channel_id.ok_or("User was in an unknown channel")?;

    if let Some(new_channel) = &new.channel_id {
        if old_channel == new_channel {
            return Err("User left and rejoined the same channel");
        }
    }

    let old_channel = old_channel
        .to_channel(&ctx.http).or(Err("Could not fetch channel data"))?
        .guild().ok_or("Channel was not in a guild")?;
    let old_channel = old_channel.read();
    let old_channel_category_id = &old_channel.category_id.ok_or("Previous channel isn't in any category")?;

    let members: Vec<Member> = old_channel.members(&ctx.cache).or(Err("Could not fetch channel members"))?;

    if members.len() != 0 {
        return Err("Channel isn't empty");
    }

    static BOB_CHANNEL_NAME: Lazy<String> = Lazy::new(|| {env::var("BOB_CHANNEL_NAME").expect("Missing BOB_CHANNEL_NAME envvar.")});

    // Find the bob channel category
    let mut bob_channel: Option<&GuildChannel> = None;
    let all_channels = guild.channels(&ctx.http).or(Err("Could not fetch guild channels"))?;
    for c in all_channels.values() {
        if c.name == (*BOB_CHANNEL_NAME) {
            bob_channel = Some(c);
            break;
        }
    }
    let bob_channel = bob_channel.ok_or("No bob channel found")?;
    let bob_category_id = &bob_channel.category_id.ok_or("No bob category found")?;

    if old_channel_category_id != bob_category_id {
        return Err("Channel isn't in the bob category");
    }

    let _ = bob_channel.say(&ctx.http, format!("🗑 Temp channel <#{}> was deleted, as it was empty.", &old_channel.id));

    info!("Deleting #{}", &old_channel.name);
    old_channel.delete(&ctx.http).or(Err("Failed to delete channel"))?;

    Ok(())
}
