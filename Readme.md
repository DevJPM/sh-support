# Secret Hitler Support Tool

## Installation (from source)

### Installing Rust

Go to [Rustup.rs](https://rustup.rs/) and follow the instructions there to install the compiler for the Rust programming language to compile the code.

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

First you need to generate the deck state. The command is `generate <lib count> <fasc count>`, e.g. `generate 6 11` to generate all possible decks containing 6 liberal and 11 fascist policy cards.

To inspect this state, you can use either the `next` or the `dist` command. The `next` command accepts a claim pattern, e.g. `next fff` and will find the probability associated with this draw in the current deck state. Note that entering `next BRB` will look for 2 blues among the next 3 cards, not for the specific ordering.

The `dist` command accepts a positive integer as input, e.g. `dist 3`, and will output the probabilities associated with all possible claim patterns for the next entered number of cards.

### Tracking and Analyzing Games

Independently of the above draw probability computations, the tool can also track and analyze gameplay information. **Card counts entered here are not used for anything but the visual graphs**. The usual flow for using this functionality goes as follows:

1. Setup the game, using the `roles <number of non-hitler fascists> <number of liberals>`, e.g. `roles 1 3` for a regular five-player game
2. Name all participants, by entering `name <seat> <name>` for each participant, e.g. `name 1 potato`
3. Track governments, by entering what happened in each government, `government <president> <chancellor> <presidential claim> <chancellor claim> [<shot person>]`, e.g. `government 3 1 rrr rr 2` to indicate president (seated #2) claims to have drawn three red policies and the chancellor (seated #1) indicated to have received two red policies and then the player seated #2 got shot (this last argument is optional). Alternatively, you can also enter the player names instead of the seat positions. A conflict is automatically registered if the president claimed at least 1 blue and the chancellor claimed no blues. A shot player is also always registered as confirmed not hitler. Further information can be registered using dedicated commands (including hard facts, more conflicts and investigations).
4. Inspect the game-state, there are multiple commands to inspect the current game state. There is the `graph` command to generate a visual representation. To inspect deduced information, the primary tools are `hitler_snipe`, `impossible_teams` and `liberal_percent` which all accept two boolean arguments (valued `true` or `false`), to indicate whether fascist-fascist conflict and aggressive hitler are seen as possible. These then compute the probabilities of players being hitler or being liberal. `impossible_teams` then finds all subsets of players which cannot possibly all be fascist at the same time.