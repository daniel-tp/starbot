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

use log::{self, info, error};
use star_realms_rs::{Challenge, Game, StarRealms};

use anyhow::Result;
use tokio::time::Instant;

struct StarRealmsSharedContainer;

struct StarRealmsShared {
    sr: StarRealms,
    game_turns: HashMap<i64, Game>,
    challenges: Vec<i64>,
    finished: Vec<i64>,
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
            finished: vec![],
            last_update: Instant::now(), //TODO: Maybe set to 0?
        };

        info!("Caching SR client");
        initial.check_turns().await;
        initial.check_challenges().await;
        initial.check_finished().await;
        info!("Finished Caching SR client");

        Ok(initial)
    }

    /// Check if any turns have updated since the last check
    /// This returns a HashMap of GameID and the username of the player whose turn it is
    async fn check_turns(&mut self) -> HashMap<i64, Game> {
        //TODO: Maybe return Game instead?
        let mut turns = HashMap::new();
        let activity = self.sr.activity().await.expect("Could not get activity");

        for game in activity.activegames {
            let turn = self.game_turns.get(&game.id);

            let which_turn = game.which_turn();

            if turn.is_none() {
                info!("Found new game: {:?}", game);
                turns.insert(game.id, game);
            } else {
                let turn = turn.unwrap();
                if turn.which_turn() != which_turn {
                    info!("Found new turn: {:?}", game);
                    turns.insert(game.id, game);
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

    async fn check_finished(&mut self) -> Vec<Game> {
        let mut finished = vec![];
        let activity = self.sr.activity().await.expect("Could not get activity");

        for game in activity.finishedgames {
            if !self.finished.contains(&game.id) {
                self.finished.push(game.id);
                info!("Found new challenge: {:?}", game);
                finished.push(game);
            }
        }

        if !finished.is_empty() {
            self.last_update = Instant::now();
        }
        finished
    }

}

struct Handler {
    looping: AtomicBool,
}

#[async_trait]
impl EventHandler for Handler {

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content.starts_with("!chal") {
            let sr_lock = {
                let data_read = ctx.data.read().await;
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
                    error!("Error sending message: {:?}", why);
                }
            }
        }
        if msg.content.to_lowercase().starts_with("!version") {
            if let Err(why) = msg
                .channel_id
                .say(&ctx.http, format!("Starbot {}", env!("CARGO_PKG_VERSION")))
                .await
            {
                error!("Error sending message: {:?}", why);
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
                                format!("Player's Turn: {} ({}) in game {} vs {} ({})", turn.1.which_turn(), turn.1.clientdata.get_auth(&turn.1.which_turn()).unwrap(), turn.0, &turn.1.opponentname, turn.1.clientdata.get_auth(&turn.1.opponentname).unwrap()),
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
                                    "{} is challenging {} to a game of Star Realms! 🚀🚀🚀",
                                    chal.challengername, chal.opponentname
                                ),
                            )
                            .await
                        {
                            println!("Error sending message: {:?}", why);
                        }
                    }

                    for fin in sr.check_finished().await {
                        if let Err(why) = ChannelId(473189734873825293)
                            .say(
                                &ctx1.http,
                                format!(
                                    "Game {} just finished, with {} at {} and {} at {}",
                                    fin.id, fin.clientdata.p1_name, fin.clientdata.p1_auth, fin.clientdata.p2_name, fin.clientdata.p2_auth
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
