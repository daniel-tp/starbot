use std::{
    collections::HashMap,
    env,
    sync::{atomic::AtomicBool, atomic::Ordering, Arc},
    time::Duration,
};

use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::*,
};

use log::{self, info};
use star_realms_rs::{Challenge, StarRealms};

use anyhow::Result;
use tokio::time::Instant;

struct StarRealmsSharedContainer;

struct StarRealmsShared {
    sr: StarRealms,
    game_turns: HashMap<i64, String>,
    challenges: Vec<i64>,
    last_update: Instant,
}

impl TypeMapKey for StarRealmsSharedContainer {
    type Value = Arc<RwLock<StarRealmsShared>>;
}

impl StarRealmsShared {
    ///Initialise the Star Realms client, and get the latest data.
    async fn new() -> Result<StarRealmsShared> {
        let mut initial = StarRealmsShared {
            sr: StarRealms::new(
                env::var("SR_USERNAME")
                    .expect("Expected SR_USERNAME env")
                    .as_str(),
                env::var("SR_PASSWORD")
                    .expect("Expected SR_PASSWORD env")
                    .as_str(),
            )
            .await?,
            game_turns: HashMap::new(),
            challenges: vec![],
            last_update: Instant::now(), //TODO: Maybe set to 0?
        };

        info!("Caching SR client");
        initial.check_turns().await;
        initial.check_challenges().await;
        info!("Finished Caching SR client");

        Ok(initial)
    }

    /// Check if any turns have updated since the last check
    /// This returns a HashMap of GameID and the username of the player whose turn it is
    async fn check_turns(&mut self) -> HashMap<i64, String> {
        //TODO: Maybe return Game instead?
        let mut turns = HashMap::new();
        let activity = self.sr.activity().await.expect("Could not get activity");

        for game in activity.activegames {
            let turn = self.game_turns.get(&game.id);

            let mut which_turn = game.opponentname.clone();
            if game.actionneeded {
                which_turn = self.sr.token.username.clone();
            }

            if turn.is_none() {
                turns.insert(game.id, which_turn);
                info!("Found new game: {:?}", game);
            } else {
                let turn = turn.unwrap();
                if turn != &which_turn {
                    turns.insert(game.id, which_turn);
                    info!("Found new turn: {:?}", game);
                } else {
                    info!("Game {} already on last found turn", game.id);
                }
            }
        }

        if !turns.is_empty() {
            self.game_turns.extend(turns.clone());
            self.last_update = Instant::now();
        }

        turns
    }

    async fn check_challenges(&mut self) -> Vec<Challenge> {
        let mut challenges = vec![];
        let activity = self.sr.activity().await.expect("Could not get activity");

        for chal in activity.challenges {
            if !self.challenges.contains(&chal.id) {
                self.challenges.push(chal.id);
                info!("Found new challenge: {:?}", chal);
                challenges.push(chal);
            }
        }

        if !challenges.is_empty() {
            self.last_update = Instant::now();
        }
        challenges
    }
}

struct Handler {
    looping: AtomicBool,
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!turn" {
            let sr_lock = {
                let data_read = ctx.data.read().await;

                // Since the CommandCounter Value is wrapped in an Arc, cloning will not duplicate the
                // data, instead the reference is cloned.
                // We wap every value on in an Arc, as to keep the data lock open for the least time possible,
                // to again, avoid deadlocking it.
                data_read
                    .get::<StarRealmsSharedContainer>()
                    .expect("Expected StarRealmsSharedContainer in TypeMap.")
                    .clone()
            };
            let mut sr = sr_lock.write().await;
            for turn in sr.check_turns().await {
                if let Err(why) = msg
                    .channel_id
                    .say(
                        &ctx.http,
                        format!("Player's Turn: {} in game {}", turn.1, turn.0),
                    )
                    .await
                {
                    println!("Error sending message: {:?}", why);
                }
            }
        }
        if msg.content.starts_with("!chal") {
            let sr_lock = {
                let data_read = ctx.data.read().await;

                // Since the CommandCounter Value is wrapped in an Arc, cloning will not duplicate the
                // data, instead the reference is cloned.
                // We wap every value on in an Arc, as to keep the data lock open for the least time possible,
                // to again, avoid deadlocking it.
                data_read
                    .get::<StarRealmsSharedContainer>()
                    .expect("Expected StarRealmsSharedContainer in TypeMap.")
                    .clone()
            };
            let sr = sr_lock.write().await;
            let activity = sr.sr.activity().await.expect("Could not get activity");
            for chal in activity.challenges {
                if let Err(why) = msg
                    .channel_id
                    .say(
                        &ctx.http,
                        format!(
                            "Challenge from: {} to {}",
                            chal.challengername, chal.opponentname
                        ),
                    )
                    .await
                {
                    println!("Error sending message: {:?}", why);
                }
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let ctx = Arc::new(ctx);
        if !self.looping.load(Ordering::Relaxed) {
            let ctx1 = Arc::clone(&ctx);

            tokio::spawn(async move {
                loop {
                    let sr_lock = {
                        let data_read = ctx.data.read().await;

                        data_read
                            .get::<StarRealmsSharedContainer>()
                            .expect("Expected StarRealmsSharedContainer in TypeMap.")
                            .clone()
                    };

                    let mut sr = sr_lock.write().await;

                    for turn in sr.check_turns().await {
                        if let Err(why) = ChannelId(473189734873825293)
                            .say(
                                &ctx1.http,
                                format!("Player's Turn: {} in game {}", turn.1, turn.0),
                            )
                            .await
                        {
                            println!("Error sending message: {:?}", why);
                        }
                    }

                    for chal in sr.check_challenges().await {
                        if let Err(why) = ChannelId(473189734873825293)
                            .say(
                                &ctx1.http,
                                format!(
                                    "New challenge from: {} to {}",
                                    chal.challengername, chal.opponentname
                                ),
                            )
                            .await
                        {
                            println!("Error sending message: {:?}", why);
                        }
                    }

                    if sr.last_update.elapsed().as_secs() >= (30 * 60) {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    } else {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            });
        }
        self.looping.swap(true, Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::builder(&token)
        .event_handler(Handler {
            looping: AtomicBool::new(false),
        })
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<StarRealmsSharedContainer>(Arc::new(RwLock::new(
            StarRealmsShared::new().await?,
        )))
    }

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }

    Ok(())
}
