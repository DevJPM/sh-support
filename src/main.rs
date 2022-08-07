use std::{collections::HashMap, result::Result};

use repl_rs::{Command, Parameter, Repl, Value};

mod deck;
mod error;
mod information;
mod players;
mod policy;
mod secret_role;

use deck::*;
use error::Error;
use players::{game_configuration::GameConfiguration, *};

//fn approx_one(value : f64) -> bool { (value - 1.0).abs() <= 1e-6 }

#[derive(Debug)]
pub struct Context {
    player_state : PlayerState
}

impl Context {
    fn invariant(&self) -> bool { self.player_state.invariant() }
}

type PlayerID = usize;

fn exit(_args : HashMap<String, Value>, _context : &mut Context) -> Result<Option<String>, Error> {
    std::process::exit(0);
}

const VERSION : &str = env!("CARGO_PKG_VERSION");

fn main() -> Result<(), Error> {
    Ok(Repl::new(Context {
        player_state : PlayerState::new(GameConfiguration::new_standard(7, false)?)
    })
    .use_completion(true)
    .with_description("Tool to assist with computational secret hitler questions.")
    .with_version(VERSION)
    .with_name("sh-tool")
    .add_command(
        Command::new("debug_decks", debug_decks)
            .with_parameter(Parameter::new("num_lib").set_required(true)?)?
            .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
            .with_help(
                "Prints out all decks with a specified amount of liberal and fascist cards."
            )
    )
    .add_command(Command::new("exit", exit).with_help("Exits this program."))
    .add_command(Command::new("quit", exit).with_help("Exits this program."))
    .add_command(
        Command::new("next", next)
            .with_parameter(Parameter::new("num_lib").set_required(true)?)?
            .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
            .with_parameter(Parameter::new("pattern").set_required(true)?)?
            .with_help(
                "Computes the probability that the next few cards of a deck with the specified \
                 amount of liberal and fascist cards match the specified card counts (order is \
                 ignored). E.g. \"next BBR\" will match \"BBR,RBB,BRB,...\" "
            )
    )
    .add_command(
        Command::new("dist", dist)
            .with_parameter(Parameter::new("num_lib").set_required(true)?)?
            .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
            .with_parameter(Parameter::new("window_size").set_required(true)?)?
            .with_help(
                "Computes the distribution of claim-like cards within the next window_size cards \
                 for a deck with the specified amount of liberal and fascist cards."
            )
    )
    .add_command(
        Command::new("standard_game", standard_game)
            .with_parameter(Parameter::new("player_count").set_required(true)?)?
            .with_parameter(Parameter::new("rebalance").set_default("true")?)?
            .with_help(
                "Configures the tracked game state for a standard game with <player_count> \
                 participants and indicating whether the SecretHitler.io rebalance is used or not \
                 (default is true)."
            )
    )
    .add_command(
        Command::new("debug_roles", debug_roles)
            .with_help("Prints out all role assignments in the current context.")
    )
    .add_command(
        Command::new("hard_fact", add_hard_fact)
            .with_parameter(Parameter::new("player_position").set_required(true)?)?
            .with_parameter(Parameter::new("role").set_required(true)?)?
            .with_help("Adds a known hard fact about a player.")
    )
    .add_command(
        Command::new("debug_filtered_roles", debug_filtered_roles)
            .with_parameter(Parameter::new("allow_fascist_fascist_conflict").set_required(true)?)?
            .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
            .with_help("Shows all the possible role assignments filtered by the fact database.")
    )
    .add_command(
        Command::new("show_manual_facts", show_facts)
            .with_help("Shows the manually added facts with indices for removal.")
    )
    .add_command(
        Command::new("known_facts", show_known_facts)
            .with_help("Shows all the information deduced about this game.")
    )
    .add_command(
        Command::new("remove_fact", remove_fact)
            .with_parameter(Parameter::new("fact_to_be_removed").set_required(true)?)?
            .with_help("Removes the fact with the given index from the database.")
    )
    .add_command(
        Command::new("conflict", add_conflict)
            .with_parameter(Parameter::new("president").set_required(true)?)?
            .with_parameter(Parameter::new("chancellor").set_required(true)?)?
            .with_help(
                "Adds a policy conflict between the president and the chancellor to the fact \
                 database."
            )
    )
    .add_command(
        Command::new("confirm_not_hitler", confirm_not_hitler)
            .with_parameter(Parameter::new("player").set_required(true)?)?
            .with_help("Confirms that the given player is not hitler.")
    )
    .add_command(
        Command::new("liberal_investigation", liberal_investigation)
            .with_parameter(Parameter::new("investigator").set_required(true)?)?
            .with_parameter(Parameter::new("investigatee").set_required(true)?)?
            .with_help(
                "Adds an investigation with a liberal result by the investigator on the \
                 investigatee."
            )
    )
    .add_command(
        Command::new("fascist_investigation", fascist_investigation)
            .with_parameter(Parameter::new("investigator").set_required(true)?)?
            .with_parameter(Parameter::new("investigatee").set_required(true)?)?
            .with_help(
                "Adds an investigation with a fascist result by the investigator on the \
                 investigatee."
            )
    )
    .add_command(
        Command::new("impossible_teams", impossible_teams)
            .with_parameter(Parameter::new("allow_fascist_fascist_conflict").set_required(true)?)?
            .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
            .with_help(
                "Identifies teams of fascists that are impossible based on the current \
                 information."
            )
    )
    .add_command(
        Command::new("hitler_snipe", hitler_snipe)
            .with_parameter(Parameter::new("allow_fascist_fascist_conflict").set_required(true)?)?
            .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
            .with_help(
                "Shows the probability of each player being hitler based on the current filtered \
                 information."
            )
    )
    .add_command(
        Command::new("liberal_percent", liberal_percent)
            .with_parameter(Parameter::new("allow_fascist_fascist_conflict").set_required(true)?)?
            .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
            .with_help(
                "Shows the probability of each player being a liberal based on the current \
                 filtered information."
            )
    )
    .add_command(
        Command::new("graph", graph)
            .with_parameter(Parameter::new("filename").set_required(true)?)?
            .with_parameter(
                Parameter::new("auto")
                    .set_required(false)?
                    .set_default("false")?
            )?
            .with_parameter(
                Parameter::new("dot-invocation")
                    .set_required(false)?
                    .set_default("")?
            )?
            .with_help(
                "Generates the graphviz graph. If \"auto\" is set to true, updates the .dot file \
                 automatically. If \"dot-invocation\" is also supplied it will also generate the \
                 .png automatically and remove the .dot file, example values include \"dot\" and \
                 \"bash\"."
            )
    )
    .add_command(
        Command::new("name", name)
            .with_parameter(Parameter::new("position").set_required(true)?)?
            .with_parameter(Parameter::new("display_name").set_required(true)?)?
            .with_help("Names a player for nicer reading.")
    )
    .add_command(
        Command::new("topdeck", topdeck)
            .with_parameter(Parameter::new("drawn_policy").set_required(true)?)?
            .with_help("Registers a top decked card with the given alignment.")
    )
    .add_command(
        Command::new("government", add_government)
            .with_parameter(Parameter::new("president").set_required(true)?)?
            .with_parameter(Parameter::new("chancellor").set_required(true)?)?
            .with_parameter(Parameter::new("presidential_blues").set_required(true)?)?
            .with_parameter(Parameter::new("chancellor_blues").set_required(true)?)?
            .with_parameter(Parameter::new("first_argument").set_default("NULL")?)?
            .with_parameter(Parameter::new("second_argument").set_default("NULL")?)?
            .with_help(
                "Logs a government with president, chancellor and claims. Conflicts are detected \
                 by the president claiming a non-0 amount of blue policies and the chancellor \
                 claiming 0. Conflicts are automatically registered for analysis."
            )
    )
    .add_command(
        Command::new("pop_government", pop_government)
            .with_help("Removes the latest government from the state.")
    )
    .add_command(
        Command::new("show_governments", show_governments)
            .with_help("Shows the currently registered governments.")
    )
    .add_command(
        Command::new("load_game_config", load_game_config)
            .with_parameter(Parameter::new("filename").set_required(true)?)?
            .with_help("Loads a custom game configuration from the indicated file.")
    )
    .add_command(
        Command::new("create_game_config", create_game_config)
            .with_parameter(Parameter::new("filename").set_required(true)?)?
            .with_help(
                "Starts a wizard to create a new game configuration and saves it to the given \
                 file. Immediately resets the current state and activates the entered \
                 configuration."
            )
    )
    .add_command(
        Command::new("shuffle_probabilities", total_draw_probability).with_help(
            "Computes the probability of the occured shuffles happening assuming nobody lied."
        )
    )
    .add_command(
        Command::new("probability_tree", probability_tree)
            .with_parameter(Parameter::new("filename").set_required(true)?)?
            .with_parameter(
                Parameter::new("auto")
                    .set_required(false)?
                    .set_default("false")?
            )?
            .with_parameter(
                Parameter::new("dot-invocation")
                    .set_required(false)?
                    .set_default("")?
            )?
            .with_help(
                "Generates the graphviz forest of probabilities for draws. If \"auto\" is set to \
                 true, updates the .dot file automatically. If \"dot-invocation\" is also \
                 supplied it will also generate the .png automatically and remove the .dot file, \
                 example values include \"dot\" and \"bash\". Red circled governments denote ones \
                 where the president lied. Red text implies further that both the president and \
                 the chancellor must have lied. The probabilities assume the path leading them to \
                 be the truth but also consider the policies passed in future draw windows \
                 without making further assumptions about them."
            )
    )
    .run()?)
}
