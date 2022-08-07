# Secret Hitler Support Tool

## Installation (from source)

### Installing Rust

Go to [Rustup.rs](https://rustup.rs/) and follow the instructions there to install the compiler for the Rust programming language to compile the code.
It will require the Visual C++ build tools if you're on Windows. The easiest way to get them is to go to the [Visual Studio download page](https://visualstudio.microsoft.com/downloads/) and then going to the "Tools" section and just downloading the builds tools.

### Installing Graphviz

Go to the [Graphviz website](https://graphviz.org/download/) and install Graphviz to your system. This is needed to generate the graphs that display the current situation.

### Creating the Executable

Open a terminal in the directory where you cloned / downloaded this repository and type in `cargo run` to just run the executable.   
If it's slow during running on your system use `cargo run --release` instead.   
If you prefer a standalone executable, replace `run` with `build`. It will then place the executable in `target/debug` or `target/release` if you used the `--release` flag and it will be named `sh-support` (with a system-appropriate filename extension).

## Installation (from Pre-Built Release)

Go to the releases section (on the right of the code on Github) and download the binary to your system and then you can run it from anywhere. 

**Installing Graphviz is still required to generate the visual graphs.**

## Usage

The tool works as a [REPL shell](https://en.wikipedia.org/wiki/Read%E2%80%93eval%E2%80%93print_loop). To get a list of commands, enter the `help` command. To get further documentation on a specific command, enter `help <command_name>`, e.g. `help government`.

### Computing Draw probabilities

To inspect the possible decks, you can use either the `next` or the `dist` command. Both commands first take `<num lib> <num fasc>` as arguments to specify the amount of liberal and fascist policies in the deck. The `next` command accepts a claim pattern, e.g. `next 6 11 fff` and will find the probability associated with this draw in a with 6 liberal and 11 fascist policies deck state. Note that entering `next 6 11 BRB` will look for 2 blues among the next 3 cards, not for the specific ordering.

The `dist` command accepts a positive integer as input, e.g. `dist 6 11 3`, and will output the probabilities associated with all possible claim patterns for the next entered number of cards.

### Tracking and Analyzing Games

Independently of the above draw probability computations, the tool can also track and analyze gameplay information. The usual flow for using this functionality goes as follows:

1. Setup the game, using the `standard_game <player_count> [<rebalance>]` command, e.g. `standard_game 5` for a regular five-player game. The SecretHitler.io rebalance is always assumed to be preferred, you have to opt-out by specifiying e.g. `standard_game 7 false`.
If instead you wish to play with custom rules as supported by SecretHitler.io you can use `create_game_config <filename>` to create a configuration file, which you can later re-use and load with `load_game_config <filename>`. 
2. Name all participants, by entering `name <seat> <name>` for each participant, e.g. `name 1 potato`
3. Track governments, by entering what happened in each government, `government <president> <chancellor> <presidential claim> <chancellor claim> [additional_argument_1] [additional_argument_2]`, e.g. `government 3 1 rrr rr 2 b` to indicate president (seated #3) claims to have drawn three red policies and the chancellor (seated #1) indicated to have received two red policies and then the player seated #2 got investigated and called a liberal. The last arguments are needed and context specific according to the board, they can be simple player identifiers for kills or special elections, a new presidential policy claim for top-deck peeks, the above format for investigations or `<policy> <true|false>` for the single card peek and potential burn. Alternatively, you can also enter the player names instead of the seat positions whenever a player name is expected.
All that can be deduced from these governments will be deduced, including conflicts, investigation implications, card draws, non-hitler confirmations, .... If you wish to, you can still register hard facts manually anyways, e.g., to account for behavior.
4. Inspect the game-state, there are multiple commands to inspect the current game state. There is the `graph` command to generate a visual representation of the player relations. To inspect deduced information, the primary tools are `hitler_snipe`, `impossible_teams` and `liberal_percent` which all accept two boolean arguments (valued `true` or `false`), to indicate whether fascist-fascist conflict and aggressive hitler are seen as possible. These then compute the probabilities of players being hitler or being liberal. `impossible_teams` then finds all subsets of players which cannot possibly all be fascist at the same time.
Additionally, there is the `probability_tree` which takes the same arguments as the `graph` command but computes probabilities for all actual draws and claims of the various previous governments.
