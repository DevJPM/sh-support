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
use players::*;

//fn approx_one(value : f64) -> bool { (value - 1.0).abs() <= 1e-6 }

#[derive(Default, Debug)]
pub struct Context {
    deck_state : DeckState,
    player_state : PlayerState
}

impl Context {
    fn invariant(&self) -> bool { self.deck_state.invariant() && self.player_state.invariant() }
}

type PlayerID = usize;

fn exit(_args : HashMap<String, Value>, _context : &mut Context) -> Result<Option<String>, Error> {
    std::process::exit(0);
}

fn main() -> Result<(), Error> {
    Ok(Repl::new(Context::default())
        .use_completion(true)
        .with_description("Tool to assist with computational secret hitler questions.")
        .with_version("0.2.0")
        .with_name("sh-tool")
        .add_command(
            Command::new("generate", generate)
                .with_parameter(Parameter::new("num_lib").set_required(true)?)?
                .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
                .with_help(
                    "Generate a deck of specified parameters and store it as the current context."
                )
        )
        .add_command(
            Command::new("debug_decks", debug_decks)
                .with_help("Prints out all decks in the current context.")
        )
        .add_command(Command::new("exit", exit).with_help("Exits this program."))
        .add_command(Command::new("quit", exit).with_help("Exits this program."))
        .add_command(
            Command::new("next", next)
                .with_parameter(Parameter::new("pattern").set_required(true)?)?
                .with_help(
                    "Computes the probability that the next few cards match the specified card \
                     counts (order is ignored). E.g. \"next BBR\" will match \"BBR,RBB,BRB,...\" "
                )
        )
        .add_command(
            Command::new("dist", dist)
                .with_parameter(Parameter::new("window_size").set_required(true)?)?
                .with_help(
                    "Computes the distribution of claim-like cards within the next window_size \
                     cards."
                )
        )
        .add_command(
            Command::new("roles", roles)
                .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
                .with_parameter(Parameter::new("num_lib").set_required(true)?)?
                .with_help(
                    "Generates all legal role assignments for num_lib + num_fasc + 1 players."
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
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Shows all the possible role assignments filtered by the fact database."
                )
        )
        .add_command(
            Command::new("show_facts", show_facts)
                .with_help("Shows the entirety of the fact database with indices for removal.")
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
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Identifies teams of fascists that are impossible based on the current \
                     information."
                )
        )
        .add_command(
            Command::new("hitler_snipe", hitler_snipe)
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Shows the probability of each player being hitler based on the current \
                     filtered information."
                )
        )
        .add_command(
            Command::new("liberal_percent", liberal_percent)
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
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
                    "Generates the graphviz graph. If \"auto\" is set to true, updates the .dot \
                     file automatically. If \"dot-invocation\" is also supplied it will also \
                     generate the .png automatically and remove the .dot file, example values \
                     include \"dot\" and \"bash\"."
                )
        )
        .add_command(
            Command::new("name", name)
                .with_parameter(Parameter::new("position").set_required(true)?)?
                .with_parameter(Parameter::new("display_name").set_required(true)?)?
                .with_help("Names a player for nicer reading.")
        )
        .add_command(
            Command::new("government", add_government)
                .with_parameter(Parameter::new("president").set_required(true)?)?
                .with_parameter(Parameter::new("chancellor").set_required(true)?)?
                .with_parameter(Parameter::new("presidential_blues").set_required(true)?)?
                .with_parameter(Parameter::new("chancellor_blues").set_required(true)?)?
                .with_parameter(
                    Parameter::new("killed_player")
                        .set_required(false)?
                        .set_default("0")?
                )?
                .with_help(
                    "Logs a government with president, chancellor and claims. Conflicts are \
                     detected by the president claiming a non-0 amount of blue policies and the \
                     chancellor claiming 0. Conflicts are automatically registered for analysis."
                )
        )
        .add_command(
            Command::new("pop_government", pop_government)
                .with_help("Removes the latest government from the state.")
        )
        .run()?)
}
