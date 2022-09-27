use std::{error::Error, fs::File};
use std::io::{self, prelude::*, BufReader};
use clap::{
    crate_description, crate_name, crate_version, value_t, value_t_or_exit, App, AppSettings, Arg,
    SubCommand,
};
use serde::{Deserialize, Serialize};
use solana_clap_utils::{
    fee_payer::fee_payer_arg,
    input_parsers::{pubkey_of, pubkey_of_signer, pubkeys_of, signer_of},
    input_validators::{is_amount, is_parsable, is_url, is_valid_pubkey, is_valid_signer},
    keypair::signer_from_path,
};
use solana_client::rpc_client::RpcClient;
use solana_program::{program_pack::Pack, pubkey::Pubkey , pubkey::ParsePubkeyError};
use solana_sdk::{
    commitment_config::CommitmentConfig, signature::Signer, transaction::Transaction,
};
use spl_associated_token_account::{create_associated_token_account, get_associated_token_address};
use wave_dist::state::Distribution;
use bs58;
use std::mem;  

const DISTRIBUTE_CHUNK_SIZE: usize = 20;
const TOKEN_ADDRESS: &str = "7yzuYZdm4MyV8E3PwMWP9i7BR68sbh83MjuRbWvDbRgv";
const DISTRIB_PROGRAM: &str = "kmKvdQWRAqekZPz4dqAdhfHBDEug4VnHs5wLyD2ybNN";

struct Config {
    rpc_client: RpcClient,
    fee_payer: Box<dyn Signer>,
    program_id: Pubkey,
}

#[derive(Serialize, Deserialize, Debug)]
struct StoredDistribution {
    pub program_id: Pubkey,
    pub project_name: String,
    pub project_pubkey:Pubkey,
    pub dist_account: Pubkey,
    pub max_recipients: u16,
    pub dist_authority: Pubkey,
    pub dist_authority_input: String,
    pub token_address: Pubkey,
    pub token_account: Pubkey,
    pub recipient_file: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct Participant {
    pub address: Pubkey
}
#[derive(Serialize, Deserialize, Debug)]
struct StoredParticipants {
    pub users: Participant
}

fn String_to_pubkey (mut str : String) -> Result<Pubkey,ParsePubkeyError> {
    //let mut a:[u8;32] = Default::default();
   
    let decoded = bs58::decode(str).into_vec().map_err(|_| ParsePubkeyError::Invalid)?;
    //a.copy_from_slice(&test.as_bytes());
    if decoded.len() != mem::size_of::<Pubkey>() {
        
        Err(ParsePubkeyError::WrongSize)
    } else {
        Ok(Pubkey::new(&decoded))
    }
    

}
fn main() -> Result<(), Box<dyn Error>> {
    solana_logger::setup_with_default("solana=info");

    let default_program_id: &str = &wave_dist::id().to_string();

    let matches = App::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg({
            let arg = Arg::with_name("config_file")
                .short("C")
                .long("config")
                .value_name("PATH")
                .takes_value(true)
                .global(true)
                .help("Configuration file to use");
            if let Some(ref config_file) = *solana_cli_config::CONFIG_FILE {
                arg.default_value(config_file)
            } else {
                arg
            }
        })
        .arg(
            Arg::with_name("program_id")
                .long("program-id")
                .value_name("ADDRESS")
                .takes_value(true)
                .global(true)
                .default_value(default_program_id)
                .validator(is_valid_pubkey)
                .help("Distribution program ID"),
        )
        .arg(
            Arg::with_name("json_rpc_url")
                .long("url")
                .value_name("URL")
                .takes_value(true)
                .validator(is_url)
                .help("JSON RPC URL for the cluster.  Default from the configuration file."),
        )
        .arg(fee_payer_arg().global(true))
        .subcommand(SubCommand::with_name("create-and-fund").about("Create a new distribution")
        .arg_from_usage("<PROJECT_NAME> 'Set the project_name for distribution'")
        .arg(
            Arg::with_name("amount")
                .long("amount")
                .validator(is_amount)
                .value_name("AMOUNT")
                .takes_value(true)
                .required(true)
                .help("The amount of tokens to fund the distribution with."),
        )
        )
        .subcommand(
            SubCommand::with_name("create-distribution")
                .about("Create a new distribution")
                .arg_from_usage("<PROJECT_NAME> 'Set the project_name for distribution'")
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                )
                .arg(
                    Arg::with_name("token")
                        .long("token")
                        .value_name("ADDRESS")
                        .validator(is_valid_pubkey)
                        .takes_value(true)
                        .required(true)
                        .help("The token mint address of the token that will be distributed."),
                )
                .arg(
                    Arg::with_name("dist_authority")
                        .long("dist-authority")
                        .alias("owner")
                        .value_name("ADDRESS")
                        .validator(is_valid_pubkey)
                        .takes_value(true)
                        .required(true)
                        .help(
                            "Specify the dist authority address. \
                             Defaults to the client keypair address.",
                        ),
                )
                .arg(
                    Arg::with_name("max_recipients")
                        .long("max-recipients")
                        .validator(is_parsable::<u16>)
                        .value_name("NUMBER")
                        .takes_value(true)
                        .required(true)
                        .help(
                            "The maximum number of recipients for the distribution. \
                             Affects the space allocated for the program account. \
                             Cannot be increased later.",
                        ),
                )
                .arg(
                    Arg::with_name("output")
                        .long("output")
                        .short("o")
                        .value_name("PATH")
                        .takes_value(true)
                        .default_value("state.json")
                        .help(
                            "Saves created distribution state to a file for \
                            easy reference.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("dist-account-from-seed")
                .about("Outputs the dist account address for a seed")
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("change-dist-authority")
                .about("Changes the distribution authority of a distribution")
                .arg(
                    Arg::with_name("state_file")
                        .long("state-file")
                        .value_name("PATH")
                        .takes_value(true)
                        .help(
                            "A state file created by create-distribution to \
                             fill out most required values automatically.",
                        ),
                )
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .conflicts_with("dist_account")
                        .conflicts_with("state_file")
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                )
                .arg(
                    Arg::with_name("dist_account")
                        .long("dist-account")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required_unless("seed")
                        .conflicts_with("state_file")
                        .help("The distribution account address."),
                )
                .arg(
                    Arg::with_name("dist_authority")
                        .long("dist-authority")
                        .value_name("KEYPAIR")
                        .validator(is_valid_signer)
                        .takes_value(true)
                        .required(true)
                        .conflicts_with("state_file")
                        .help("The current account with distribution authority."),
                )
                .arg(
                    Arg::with_name("new_dist_authority")
                        .long("new-dist-authority")
                        .value_name("KEYPAIR")
                        .validator(is_valid_signer)
                        .takes_value(true)
                        .required(true)
                        .help("The new account with distribution authority."),
                ),
        )
        .subcommand(
            SubCommand::with_name("show-distribution")
                .about("Shows the state of a distribution")
                .arg(
                    Arg::with_name("state_file")
                        .long("state-file")
                        .value_name("PATH")
                        .takes_value(true)
                        .help(
                            "A state file created by create-distribution to \
                             fill out most required values automatically.",
                        ),
                )
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .conflicts_with("dist_account")
                        .conflicts_with("state_file")
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                )
                .arg(
                    Arg::with_name("dist_account")
                        .long("dist-account")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required_unless("seed")
                        .conflicts_with("state_file")
                        .help("The distribution account address."),
                )
        )
        .subcommand(
            SubCommand::with_name("fund-distribution")
                .about("Funds a distribution")
                .arg(
                    Arg::with_name("state_file")
                        .long("state-file")
                        .value_name("PATH")
                        .takes_value(true)
                        .help(
                            "A state file created by create-distribution to \
                             fill out most required values automatically.",
                        ),
                )
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .conflicts_with("dist_account")
                        .conflicts_with("state_file")
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                )
                .arg(
                    Arg::with_name("dist_account")
                        .long("dist-account")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required_unless("seed")
                        .conflicts_with("state_file")
                        .help("The distribution account address."),
                )
                .arg(
                    Arg::with_name("amount")
                        .long("amount")
                        .validator(is_amount)
                        .value_name("AMOUNT")
                        .takes_value(true)
                        .required(true)
                        .help("The amount of tokens to fund the distribution with."),
                )
                .arg(
                    Arg::with_name("token")
                        .long("token")
                        .value_name("ADDRESS")
                        .validator(is_valid_pubkey)
                        .takes_value(true)
                        .required(true)
                        .conflicts_with("state_file")
                        .help(
                            "The token mint address of the token that will fund the distribution.",
                        ),
                )
                .arg(
                    Arg::with_name("funder")
                        .long("funder")
                        .value_name("KEYPAIR")
                        .validator(is_valid_signer)
                        .takes_value(true)
                        .required(true)
                        .help("The account that will fund the distribution."),
                ),
        )
        .subcommand(
            SubCommand::with_name("begin-distribution")
                .about("Begins a distribution, finalizing the number of recipients")
                .arg(
                    Arg::with_name("state_file")
                        .long("state-file")
                        .required(true)
                        .value_name("PATH")
                        .takes_value(true)
                        .help(
                            "A state file created by create-distribution to \
                             fill out most required values automatically.",
                        ),
                )
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .conflicts_with("dist_account")
                        .conflicts_with("state_file")
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                )
                .arg(
                    Arg::with_name("dist_account")
                        .long("dist-account")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required_unless("seed")
                        .conflicts_with("state_file")
                        .help("The distribution account address."),
                )
                .arg(
                    Arg::with_name("dist_authority")
                        .long("dist-authority")
                        .value_name("KEYPAIR")
                        .validator(is_valid_signer)
                        .takes_value(true)
                        .required(true)
                        .conflicts_with("state_file")
                        .help("The account with distribution authority."),
                )
                .arg(
                    Arg::with_name("num_recipients")
                        .long("num-recipients")
                        .validator(is_parsable::<u16>)
                        .value_name("NUMBER")
                        .takes_value(true)
                        .conflicts_with("state_file")
                        .required(true)
                        .help(
                            "The final number of recipients for the distribution. \
                             The distribution will complete once this many \
                             recipients have been distributed to.",
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("distribute")
                .about("Performs a distribution. May be repeated until fully distributed.")
                .arg(
                    Arg::with_name("state_file")
                        .long("state-file")
                        .value_name("PATH")
                        .takes_value(true)
                        .help(
                            "A state file created by create-distribution to \
                             fill out most required values automatically.",
                        ),
                )
                .arg(
                    Arg::with_name("seed")
                        .long("seed")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .conflicts_with("dist_account")
                        .conflicts_with("state_file")
                        .help(
                            "A pubkey that programmatically determines the \
                             address of the distribution account.",
                        ),
                )
                .arg(
                    Arg::with_name("dist_account")
                        .long("dist-account")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required_unless("seed")
                        .conflicts_with("state_file")
                        .help("The distribution account address."),
                )
                .arg(
                    Arg::with_name("dist_authority")
                        .long("dist-authority")
                        .value_name("KEYPAIR")
                        .validator(is_valid_signer)
                        .takes_value(true)
                        .required(true)
                        .conflicts_with("state_file")
                        .help("The account with distribution authority."),
                )
                .arg(
                    Arg::with_name("token")
                        .long("token")
                        .value_name("ADDRESS")
                        .validator(is_valid_pubkey)
                        .takes_value(true)
                        .required(true)
                        .conflicts_with("state_file")
                        .help(
                            "The token mint address of the token that will fund the distribution.",
                        ),
                )
                .arg(
                    Arg::with_name("recipienst_file")
                        .long("recipient")
                        .validator(is_valid_pubkey)
                        .value_name("ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .conflicts_with("state_file")
                        .multiple(true)
                        .help("A recipient file to distribute to."),
                )
                .arg(
                    Arg::with_name("skip")
                        .long("skip")
                        .value_name("COUNT")
                        .takes_value(true)
                        .default_value("0")
                        .help("Skips the specified number of recipients. Useful to continue from failure."),
                ),
        )
        .get_matches();

    let mut wallet_manager = None;

    let cli_config = if let Some(config_file) = matches.value_of("config_file") {
        solana_cli_config::Config::load(config_file).unwrap_or_default()
    } else {
        solana_cli_config::Config::default()
    };

    let config = {
        let json_rpc_url = value_t!(matches, "json_rpc_url", String)
            .unwrap_or_else(|_| cli_config.json_rpc_url.clone());

        let fee_payer = signer_from_path(
            &matches,
            matches
                .value_of("fee_payer")
                .unwrap_or(&cli_config.keypair_path),
            "fee_payer",
            &mut wallet_manager,
        )?;
        let program_id = String_to_pubkey(DISTRIB_PROGRAM.to_string())?;
        Config {
            rpc_client: RpcClient::new_with_commitment(json_rpc_url, CommitmentConfig::confirmed()),
            fee_payer,
            program_id,
        }
    };

    match matches.subcommand() {
        ("create-and-fund", Some(arg_matches)) => {
            // default seed to wallet
            let seed = config.fee_payer.pubkey();
            let token_address = String_to_pubkey(TOKEN_ADDRESS.to_string())?;
            let dist_authority = config.fee_payer.pubkey();
            let dist_authority_input: &String = &config.fee_payer.pubkey().to_string();
            let mut project_name = value_t_or_exit!(arg_matches, "PROJECT_NAME",String);
            let participants_file_path = project_name.to_owned() + ".txt";
            let output_path = project_name.to_owned() + ".state";
            let output_file = File::create(&output_path)?;
            let ui_amount = value_t_or_exit!(arg_matches, "amount", f64);
            // read user list
            //let file = File::open(participants_file_path.clone())?;
            //let reader = BufReader::new(file);
            //let mut participants = vec![];
            // Reading line by line
            /*let mut count : u16 = 0;
            for line in reader.lines() {
                count = count +1 ;
                participants.push(Participant{address: String_to_pubkey(line?)?});
                
            }
            */
            let max_recipients = 500; // count
            let  saved_projectname = project_name.clone() ;
            // Convert project_name to pubkey size
            if project_name.len() < mem::size_of::<Pubkey>() {
        
                while project_name.len() != mem::size_of::<Pubkey>() {
                    project_name = project_name + "0";
                }
                
            }
            let encoded = bs58::encode(project_name.clone()).into_string();
        
            let project_pubkey= String_to_pubkey(encoded)?;
            let (dist_account, seed_bump) =
                Pubkey::find_program_address(&[seed.as_ref(),project_pubkey.as_ref()], &config.program_id);

            let dist_token_account = get_associated_token_address(&dist_account, &token_address);

            println!("Creating distribution {}", dist_account);
            println!("  Program ID: {}", config.program_id);
            println!("  Max recipients: {}", max_recipients);
            println!("  Dist authority: {}", dist_authority);
            println!("  Token address: {}", token_address);
            println!("  Token account: {}", dist_token_account);
            println!("  Fee payer: {}", config.fee_payer.pubkey());
            println!("Projectname to pubkey {}", project_name);

            let dist_json = StoredDistribution {
                program_id: config.program_id,
                project_name: saved_projectname,
                project_pubkey: project_pubkey,
                dist_account,
                max_recipients,
                dist_authority,
                dist_authority_input: dist_authority_input.into(),
                token_address,
                token_account: dist_token_account,
                recipient_file: participants_file_path
            };

            let instructions = vec![
                wave_dist::instruction::init_distribution(
                    &config.program_id,
                    &token_address,
                    &dist_account,
                    &config.fee_payer.pubkey(),
                    &seed,
                    &project_pubkey,
                    seed_bump,
                    max_recipients,
                    &dist_authority,
                ),
                create_associated_token_account(
                    &config.fee_payer.pubkey(),
                    &dist_account,
                    &token_address,
                ),
            ];

            let mut transaction =
                Transaction::new_with_payer(&instructions, Some(&config.fee_payer.pubkey()));

            let signers = vec![config.fee_payer.as_ref()];

            let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
            transaction.sign(&signers, recent_blockhash);

            config
                .rpc_client
                .send_and_confirm_transaction_with_spinner(&transaction)?;

            serde_json::to_writer_pretty(&output_file, &dist_json)?;
            println!("Creation Success!");
            println!();
            println!("Wrote state to {}", &output_path);
            
            
            let funder_token_account =
                get_associated_token_address(&config.fee_payer.pubkey(), &token_address);
               
            let funder_token_account_on_chain = config
                .rpc_client
                .get_token_account(&funder_token_account)?.unwrap();
              
            let amount = spl_token::ui_amount_to_amount(
                ui_amount,
                funder_token_account_on_chain.token_amount.decimals,
            );
            println!("Funding distribution {}", dist_account);
            println!("  Program ID: {}", config.program_id);
            println!("  Token address: {}", token_address);
            println!("  Funding account: {}", config.fee_payer.pubkey());
            println!("  Funding token account: {}", funder_token_account);
            println!("  Amount: {}", ui_amount);
            println!("  Amount (base units): {}", amount);
            println!("  Token account: {}", dist_token_account);
            println!("  Fee payer: {}", config.fee_payer.pubkey());
            println!();

            let instructions = vec![wave_dist::instruction::fund_distribution(
                &config.program_id,
                &config.fee_payer.pubkey(),
                &funder_token_account,
                &dist_account,
                &dist_token_account,
                amount,
            )];

            let mut transaction =
                Transaction::new_with_payer(&instructions, Some(&config.fee_payer.pubkey()));

            let signers = vec![config.fee_payer.as_ref(),config.fee_payer.as_ref()];

            let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
            transaction.sign(&signers, recent_blockhash);

            config
                .rpc_client
                .send_and_confirm_transaction_with_spinner(&transaction)?;

            println!("Success!"); 


        }
        ("create-distribution", Some(arg_matches)) => {
            let seed = pubkey_of(arg_matches, "seed").unwrap();
            let max_recipients = value_t_or_exit!(arg_matches, "max_recipients", u16);
            let token_address = pubkey_of(arg_matches, "token").unwrap();
            let dist_authority_input = arg_matches.value_of("dist_authority").unwrap();
            let dist_authority = pubkey_of(arg_matches, "dist_authority").unwrap();
            let output_path = value_t_or_exit!(arg_matches, "output", String);
            let output_file = File::create(&output_path)?;
            let mut project_name = value_t_or_exit!(arg_matches, "PROJECT_NAME",String);
            let participants_file_path = project_name.to_owned() + ".txt";

            // Seed can be replaced by user pubkey 
            // 
            let (dist_account, seed_bump) =
                Pubkey::find_program_address(&[seed.as_ref()], &config.program_id);

            let dist_token_account = get_associated_token_address(&dist_account, &token_address);

            println!("Creating distribution {}", dist_account);
            println!("  Program ID: {}", config.program_id);
            println!("  Max recipients: {}", max_recipients);
            println!("  Dist authority: {}", dist_authority);
            println!("  Token address: {}", token_address);
            println!("  Token account: {}", dist_token_account);
            println!("  Fee payer: {}", config.fee_payer.pubkey());
            println!();
             // Convert project_name to pubkey size
             if project_name.len() < mem::size_of::<Pubkey>() {
        
                while project_name.len() != mem::size_of::<Pubkey>() {
                    project_name = project_name + "0";
                }
                
            }
            let encoded = bs58::encode(project_name.clone()).into_string();
        
            let project_pubkey= String_to_pubkey(encoded)?;

            let dist_json = StoredDistribution {
                program_id: config.program_id,
                project_name: project_name,
                project_pubkey: project_pubkey,
                dist_account,
                max_recipients,
                dist_authority,
                dist_authority_input: dist_authority_input.to_owned(),
                token_address,
                token_account: dist_token_account,
                recipient_file: participants_file_path
            };
            let project_name = value_t_or_exit!(arg_matches, "PROJECT_NAME",String);
            let project_pubkey = String_to_pubkey(project_name)?;
            let instructions = vec![
                wave_dist::instruction::init_distribution(
                    &config.program_id,
                    &token_address,
                    &dist_account,
                    &config.fee_payer.pubkey(),
                    &seed,
                    &project_pubkey,
                    seed_bump,
                    max_recipients,
                    &dist_authority,
                ),
                create_associated_token_account(
                    &config.fee_payer.pubkey(),
                    &dist_account,
                    &token_address,
                ),
            ];

            let mut transaction =
                Transaction::new_with_payer(&instructions, Some(&config.fee_payer.pubkey()));

            let signers = vec![config.fee_payer];

            let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
            transaction.sign(&signers, recent_blockhash);

            config
                .rpc_client
                .send_and_confirm_transaction_with_spinner(&transaction)?;

            serde_json::to_writer_pretty(&output_file, &dist_json)?;
            println!("Success!");
            println!();
            println!("Wrote state to {}", &output_path);
        }
        ("dist-account-from-seed", Some(arg_matches)) => {
            let seed = pubkey_of(arg_matches, "seed").unwrap();
            let (dist_account, _) =
                Pubkey::find_program_address(&[seed.as_ref()], &config.program_id);
            println!("{}", dist_account);
        }
        ("show-distribution", Some(arg_matches)) => {
            let state_file_path = arg_matches.value_of("state_file");

            let saved_state: Option<StoredDistribution> =
                if let Some(state_file_path) = state_file_path {
                    serde_json::from_reader(&File::open(state_file_path)?)?
                } else {
                    None
                };

            let dist_account = if let Some(saved_state) = &saved_state {
                saved_state.dist_account
            } else {
                let seed = pubkey_of(arg_matches, "seed");
                let dist_account = pubkey_of(arg_matches, "dist_account");
                let dist_account = match dist_account {
                    Some(pubkey) => pubkey,
                    None => {
                        let (dist_account, _) = Pubkey::find_program_address(
                            &[seed.unwrap().as_ref()],
                            &config.program_id,
                        );
                        dist_account
                    }
                };
                dist_account
            };

            let dist_account_on_chain = config.rpc_client.get_account(&dist_account)?;

            let dist = Distribution::unpack(&dist_account_on_chain.data)?;

            let token_address = dist.token();

            let dist_token_account = if let Some(saved_state) = &saved_state {
                saved_state.token_account
            } else {
                get_associated_token_address(&dist_account, &token_address)
            };

            let dist_token_account_on_chain = config
                .rpc_client
                .get_token_account(&dist_token_account)?
                .expect("dist token account does not exist");

            let ui_funded_amount = spl_token::amount_to_ui_amount(
                dist.funded_amount(),
                dist_token_account_on_chain.token_amount.decimals,
            );

            let ui_recipient_share = spl_token::amount_to_ui_amount(
                dist.recipient_share(),
                dist_token_account_on_chain.token_amount.decimals,
            );

            println!("Distribution {}", dist_account);
            println!("  Dist authority: {}", dist.dist_authority());
            println!("  Token address: {}", dist.token());
            println!("  Max recipients: {}", dist.max_recipients());
            println!("  Has started: {}", dist.has_started());
            println!("  Num recipients: {}", dist.num_recipients());
            println!("  Funded amount: {}", ui_funded_amount);
            println!("  Funded amount (base units): {}", dist.funded_amount());
            println!("  Sent recipients: {}", dist.sent_recipients());
            println!("  Recipient share: {}", ui_recipient_share);
            println!("  Recipient share (base units): {}", dist.recipient_share());
        }
        ("fund-distribution", Some(arg_matches)) => {
            let state_file_path = arg_matches.value_of("state_file");

            let saved_state: Option<StoredDistribution> =
                if let Some(state_file_path) = state_file_path {
                    serde_json::from_reader(&File::open(state_file_path)?)?
                } else {
                    None
                };

            let token_address = if let Some(saved_state) = &saved_state {
                saved_state.token_address
            } else {
                pubkey_of(arg_matches, "token").unwrap()
            };

            let dist_account = if let Some(saved_state) = &saved_state {
                saved_state.dist_account
            } else {
                let seed = pubkey_of(arg_matches, "seed");
                let dist_account = pubkey_of(arg_matches, "dist_account");
                let dist_account = match dist_account {
                    Some(pubkey) => pubkey,
                    None => {
                        let (dist_account, _) = Pubkey::find_program_address(
                            &[seed.unwrap().as_ref()],
                            &config.program_id,
                        );
                        dist_account
                    }
                };
                dist_account
            };

            let dist_token_account = if let Some(saved_state) = &saved_state {
                saved_state.token_account
            } else {
                get_associated_token_address(&dist_account, &token_address)
            };

            let (funder, _) = signer_of(arg_matches, "funder", &mut wallet_manager)?;
            let funder = funder.unwrap();
            let funder_token_account =
                get_associated_token_address(&funder.pubkey(), &token_address);

            let ui_amount = value_t_or_exit!(arg_matches, "amount", f64);

            let funder_token_account_on_chain = config
                .rpc_client
                .get_token_account(&funder_token_account)?
                .expect("funder token account does not exist");

            let amount = spl_token::ui_amount_to_amount(
                ui_amount,
                funder_token_account_on_chain.token_amount.decimals,
            );

            println!("Funding distribution {}", dist_account);
            println!("  Program ID: {}", config.program_id);
            println!("  Token address: {}", token_address);
            println!("  Funding account: {}", funder.pubkey());
            println!("  Funding token account: {}", funder_token_account);
            println!("  Amount: {}", ui_amount);
            println!("  Amount (base units): {}", amount);
            println!("  Token account: {}", dist_token_account);
            println!("  Fee payer: {}", config.fee_payer.pubkey());
            println!();

            let instructions = vec![wave_dist::instruction::fund_distribution(
                &config.program_id,
                &funder.pubkey(),
                &funder_token_account,
                &dist_account,
                &dist_token_account,
                amount,
            )];

            let mut transaction =
                Transaction::new_with_payer(&instructions, Some(&config.fee_payer.pubkey()));

            let signers = vec![config.fee_payer, funder];

            let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
            transaction.sign(&signers, recent_blockhash);

            config
                .rpc_client
                .send_and_confirm_transaction_with_spinner(&transaction)?;

            println!("Success!");
        }
        ("change-dist-authority", Some(arg_matches)) => {
            let state_file_path = arg_matches.value_of("state_file");

            let saved_state: Option<StoredDistribution> =
                if let Some(state_file_path) = state_file_path {
                    serde_json::from_reader(&File::open(state_file_path)?)?
                } else {
                    None
                };

            let dist_account = if let Some(saved_state) = &saved_state {
                saved_state.dist_account
            } else {
                let seed = pubkey_of(arg_matches, "seed");
                let dist_account = pubkey_of(arg_matches, "dist_account");
                let dist_account = match dist_account {
                    Some(pubkey) => pubkey,
                    None => {
                        let (dist_account, _) = Pubkey::find_program_address(
                            &[seed.unwrap().as_ref()],
                            &config.program_id,
                        );
                        dist_account
                    }
                };
                dist_account
            };

            let dist_authority = if let Some(saved_state) = &saved_state {
               /* let dist_authority = signer_from_path(
                    arg_matches,
                    &saved_state.dist_authority_input,
                    "dist_authority",
                    &mut wallet_manager,
                )?;
                if saved_state.dist_authority.ne(&dist_authority.pubkey()) {
                    panic!("state file's dist authority input {} does not lead to intended dist authority {}", saved_state.dist_authority_input, saved_state.dist_authority);
                }*/
                saved_state.dist_authority
            } else {
               let (dist_authority, _) =
                    signer_of(arg_matches, "dist_authority", &mut wallet_manager)?;
                let dist_authority = dist_authority.unwrap();
                
                dist_authority.pubkey()
            };
            
            let new_dist_authority =
                pubkey_of_signer(arg_matches, "new_dist_authority", &mut wallet_manager)?.unwrap();

            println!("Modifying distribution {}", dist_account);
            println!("  Program ID: {}", config.program_id);
            println!("  Current dist authority: {}", dist_authority);
            println!("  New dist authority: {}", new_dist_authority);
            println!("  Fee payer: {}", config.fee_payer.pubkey());
            println!();

            let instructions = vec![wave_dist::instruction::set_dist_authority(
                &config.program_id,
                &dist_account,
                &dist_authority,
                &new_dist_authority,
            )];

            let mut transaction =
                Transaction::new_with_payer(&instructions, Some(&config.fee_payer.pubkey()));

            let signers = vec![config.fee_payer];

            let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
            transaction.sign(&signers, recent_blockhash);

            config
                .rpc_client
                .send_and_confirm_transaction_with_spinner(&transaction)?;

            println!("Success!");
        }
        ("begin-distribution", Some(arg_matches)) => {
            let state_file_path = value_t_or_exit!(arg_matches,"state_file",String);
            
            let saved_state: Option<StoredDistribution> = serde_json::from_reader(&File::open(state_file_path)?)?;
            
            
            let dist_account = if let Some(saved_state) = &saved_state {
                saved_state.dist_account
            } else {
                let seed = pubkey_of(arg_matches, "seed");
                let dist_account = pubkey_of(arg_matches, "dist_account");
                let mut project_name = value_t_or_exit!(arg_matches, "project_name",String);
                  // Convert project_name to pubkey size
                if project_name.len() < mem::size_of::<Pubkey>() {
            
                    while project_name.len() != mem::size_of::<Pubkey>() {
                        project_name = project_name + "0";
                    }
                    
                }
                let encoded = bs58::encode(project_name.clone()).into_string();
                let project_pubkey= String_to_pubkey(encoded)?;
                let dist_account = match dist_account {
                    Some(pubkey) => pubkey,
                    None => {
                        let (dist_account, _) = Pubkey::find_program_address(
                            &[seed.unwrap().as_ref(), project_pubkey.as_ref()],

                            &config.program_id,
                        );
                        dist_account
                    }
                };
                dist_account
            };

            let num_recipients =if let Some(saved_state) = &saved_state {
                let debugstring : String = saved_state.project_name.clone() + ".txt";
                println!("{:?}",debugstring);

                let file = File::open(saved_state.project_name.clone() + ".txt")?;
                let reader = BufReader::new(file);
                let mut count : u16 = 0;
                for line in reader.lines() {
                count = count +1 ;
                }
                count
            } else {
                value_t_or_exit!(arg_matches, "num_recipients", u16)
            };
            
            
            println!("Begin distribution");
            println!("Error distribution");
            let dist_authority = if let Some(saved_state) = &saved_state {
               /* let dist_authority = signer_from_path(
                    arg_matches,
                    &saved_state.dist_authority_input,
                    "dist_authority",
                    &mut wallet_manager,
                )?;
                if saved_state.dist_authority.ne(&dist_authority.pubkey()) {
                    panic!("state file's dist authority input {} does not lead to intended dist authority {}", saved_state.dist_authority_input, saved_state.dist_authority);
                }*/
                saved_state.dist_authority
            } else {
                let (dist_authority, _) =
                    signer_of(arg_matches, "dist_authority", &mut wallet_manager)?;
                let dist_authority = dist_authority.unwrap();
                dist_authority.pubkey()
            };

            println!("Begin distribution {}", dist_account);
            println!("  Program ID: {}", config.program_id);
            println!("  Dist authority: {}", dist_authority);
            println!("  Number of recipients: {}", num_recipients);
            println!("  Fee payer: {}", config.fee_payer.pubkey());
            println!();

            let instructions = vec![wave_dist::instruction::begin_distribution(
                &config.program_id,
                &dist_account,
                &dist_authority,
                num_recipients,
            )];

            let mut transaction =
                Transaction::new_with_payer(&instructions, Some(&config.fee_payer.pubkey()));

            let signers = vec![config.fee_payer];

            let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
            transaction.sign(&signers, recent_blockhash);

            config
                .rpc_client
                .send_and_confirm_transaction_with_spinner(&transaction)?;

            println!("Success!");
        }
        ("distribute", Some(arg_matches)) => {
            let state_file_path = arg_matches.value_of("state_file");

            let saved_state: Option<StoredDistribution> =
                if let Some(state_file_path) = state_file_path {
                    serde_json::from_reader(&File::open(state_file_path)?)?
                } else {
                    None
                };

            let token_address = if let Some(saved_state) = &saved_state {
                saved_state.token_address
            } else {
                pubkey_of(arg_matches, "token").unwrap()
            };

            let dist_account = if let Some(saved_state) = &saved_state {
                println!("Ok");
                saved_state.dist_account
                
            } else {
                let seed = pubkey_of(arg_matches, "seed");
                let dist_account = pubkey_of(arg_matches, "dist_account");
                let mut project_name = value_t_or_exit!(arg_matches, "project_name",String);
                  // Convert project_name to pubkey size
                if project_name.len() < mem::size_of::<Pubkey>() {
            
                    while project_name.len() != mem::size_of::<Pubkey>() {
                        project_name = project_name + "0";
                    }
                    
                }
                let encoded = bs58::encode(project_name.clone()).into_string();
                let project_pubkey= String_to_pubkey(encoded)?;
                let dist_account = match dist_account {
                    Some(pubkey) => pubkey,
                    None => {
                        let (dist_account, _) = Pubkey::find_program_address(
                            &[seed.unwrap().as_ref(), project_pubkey.as_ref()],

                            &config.program_id,
                        );
                        dist_account
                    }
                };
                dist_account
            };

            let dist_token_account = if let Some(saved_state) = &saved_state {
                saved_state.token_account
            } else {
                get_associated_token_address(&dist_account, &token_address)
            };

            let dist_authority = if let Some(saved_state) = &saved_state {
                /*let dist_authority = signer_from_path(
                    arg_matches,
                    &saved_state.dist_authority_input,
                    "dist_authority",
                    &mut wallet_manager,
                )?;
                if saved_state.dist_authority.ne(&dist_authority.pubkey()) {
                    panic!("state file's dist authority input {} does not lead to intended dist authority {}", saved_state.dist_authority_input, saved_state.dist_authority);
                }*/
                saved_state.dist_authority
            } else {
                let (dist_authority, _) =
                    signer_of(arg_matches, "dist_authority", &mut wallet_manager)?;
                let dist_authority = dist_authority.unwrap();
                dist_authority.pubkey()
            };

            let dist_authority_pubkey = dist_authority;

            let participants_file_path = if let Some(saved_state) = saved_state {
                saved_state.recipient_file
            } else {
                let recipient_file = value_t_or_exit!(arg_matches, "recipients_file", String);
                recipient_file
            };

            // read user list
            let file = File::open(participants_file_path)?;
            let reader = BufReader::new(file);
            let mut participants = vec![];

            
            // Reading line by line
            let mut count : u16 = 0;
            for line in reader.lines() {
                count = count +1 ;
                participants.push(Participant{address: String_to_pubkey(line?)?});
                
            }
            
            let max_recipients = count;
           

            let skip = value_t_or_exit!(arg_matches, "skip", usize);

            let mut recipient_token_accounts = Vec::new();

            for recipient in &participants[skip..] {
                let recipient_token_account =
                    get_associated_token_address(&recipient.address, &token_address);
                recipient_token_accounts.push(recipient_token_account);
            }

            let fee_payer_pubkey = config.fee_payer.pubkey();

            let signers = vec![config.fee_payer];

            let recipient_token_accounts_chunks = recipient_token_accounts
                .as_slice()
                .chunks(DISTRIBUTE_CHUNK_SIZE);

            for (i, recipient_token_accounts_chunk) in recipient_token_accounts_chunks.enumerate() {
                println!(
                    "Distributing {} (recipients {}..{})",
                    dist_account,
                    1 + skip + i * DISTRIBUTE_CHUNK_SIZE,
                    skip + (i + 1) * DISTRIBUTE_CHUNK_SIZE,
                );
                println!("  Program ID: {}", config.program_id);
                println!("  Dist authority: {}", &dist_authority_pubkey);
                println!("  Skip index: {}", skip + DISTRIBUTE_CHUNK_SIZE * i);
                println!("  Recipients:");
                for recipient in &participants {
                    println!("    {}", recipient.address);
                }
                println!("  Fee payer: {}", &fee_payer_pubkey);
                println!("  Dist Authority: {}", &fee_payer_pubkey);
                println!();

                let instructions = vec![wave_dist::instruction::distribute(
                    &config.program_id,
                    &dist_account,
                    &dist_authority_pubkey,
                    &dist_token_account,
                    &recipient_token_accounts_chunk
                        .iter()
                        .collect::<Vec<&Pubkey>>(),
                )];

                let mut transaction =
                    Transaction::new_with_payer(&instructions, Some(&fee_payer_pubkey));

                let recent_blockhash = config.rpc_client.get_latest_blockhash()?;
                transaction.sign(&signers, recent_blockhash);

                config
                    .rpc_client
                    .send_and_confirm_transaction_with_spinner(&transaction)?;
            }

            println!("Success!");
        }
        _ => unreachable!(),
    }

    Ok(())
}
