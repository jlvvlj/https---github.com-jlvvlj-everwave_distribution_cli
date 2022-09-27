//! Program state processor

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_memory::sol_memcmp,
    program_pack::IsInitialized,
    program_pack::Pack,
    pubkey::{Pubkey, PUBKEY_BYTES},
    system_instruction,
    sysvar::{rent::Rent, Sysvar},
};

use crate::{
    error::DistError,
    instruction::DistInstruction,
    state::{Distribution, PdaSeed},
};

/// Processes a [DistInstruction](enum.DistInstruction.html).
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    input: &[u8],
) -> ProgramResult {
    let instruction = DistInstruction::unpack(input)?;

    match instruction {
        DistInstruction::InitializeDistribution {
            ref seed,
            ref project_name,
            seed_bump,
            max_recipients,
            ref dist_authority,
        } => {
            msg!("Instruction: InitializeDistribution");
            process_initialize_distribution(
                program_id,
                accounts,
                seed,
                project_name,
                seed_bump,
                max_recipients,
                dist_authority,
            )
        }
        DistInstruction::FundDistribution { amount } => {
            msg!("Instruction: FundDistribution");
            process_fund_distribution(program_id, accounts, amount)
        }
        DistInstruction::SetDistAuthority {
            ref new_dist_authority,
        } => {
            msg!("Instruction: SetDistAuthority");
            process_set_dist_authority(program_id, accounts, new_dist_authority)
        }
        DistInstruction::BeginDistribution { num_recipients } => {
            msg!("Instruction: BeginDistribution");
            process_begin_distribution(program_id, accounts, num_recipients)
        }
        DistInstruction::Distribute => {
            msg!("Instruction: Distribute");
            process_distribute(program_id, accounts)
        }
    }
}

/// Processes an [InitializeDistribution](enum.DistInstruction.html) instruction.
fn process_initialize_distribution(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    seed: &Pubkey,
    project_name: &Pubkey,
    seed_bump: u8,
    max_recipients: u16,
    dist_authority: &Pubkey,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let fee_payer_info = next_account_info(accounts_iter)?;
    let system_program_info = next_account_info(accounts_iter)?;

    let token_program_id = next_account_info(accounts_iter)?;
    spl_token::check_program_account(token_program_id.key)?;

    let token_info = next_account_info(accounts_iter)?;
    // TODO check owner

    let dist_account_info = next_account_info(accounts_iter)?;
    let rent = Rent::get()?;

    if !fee_payer_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let pda_seed = PdaSeed::new(*seed,*project_name, seed_bump);

    let dist_account_pubkey = pda_seed.create_pubkey(program_id)?;
    if !cmp_pubkeys(dist_account_info.key, &dist_account_pubkey) {
        return Err(ProgramError::InvalidSeeds);
    }

    let state_size = Distribution::LEN; // TODO

    let create_pda_account = system_instruction::create_account(
        fee_payer_info.key,
        dist_account_info.key,
        rent.minimum_balance(state_size),
        state_size as u64,
        program_id,
    );

    invoke_signed(
        &create_pda_account,
        &[
            system_program_info.clone(),
            fee_payer_info.clone(),
            dist_account_info.clone(),
        ],
        &[&[pda_seed.seed.as_ref(),pda_seed.project_name.as_ref() ,&[pda_seed.bump]]],
    )?;

    let mut dist = Distribution::unpack_unchecked(&dist_account_info.data.borrow())?;

    if dist.is_initialized() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    dist.init(
        pda_seed,
        *dist_authority,
        *token_info.key,
        max_recipients,
        0,
    );

    Distribution::pack(dist, &mut dist_account_info.data.borrow_mut())?;

    Ok(())
}

fn process_fund_distribution(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let source_account_info = next_account_info(accounts_iter)?;
    if !source_account_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let source_token_account_info = next_account_info(accounts_iter)?;

    let dist_account_info = next_account_info(accounts_iter)?;
    if !cmp_pubkeys(program_id, dist_account_info.owner) {
        return Err(ProgramError::IncorrectProgramId);
    }

    let dist_token_account_info = next_account_info(accounts_iter)?;

    let token_program_id = next_account_info(accounts_iter)?;
    spl_token::check_program_account(token_program_id.key)?;

    if !cmp_pubkeys(source_token_account_info.owner, token_program_id.key) {
        return Err(ProgramError::InvalidArgument);
    }

    if !cmp_pubkeys(dist_token_account_info.owner, token_program_id.key) {
        return Err(ProgramError::InvalidArgument);
    }

    let mut dist = Distribution::unpack(&dist_account_info.data.borrow())?;

    let pda_pubkey = dist.pda_seed().create_pubkey(program_id)?;

    if !cmp_pubkeys(dist_account_info.key, &pda_pubkey) {
        return Err(ProgramError::InvalidAccountData);
    }

    // TODO: For safety, check that dist_token_account_info matches
    // dist_account_info's token account.

    let pull_tokens = spl_token::instruction::transfer(
        token_program_id.key,
        source_token_account_info.key,
        dist_token_account_info.key,
        source_account_info.key,
        &[],
        amount,
    )?;

    invoke(
        &pull_tokens,
        &[
            token_program_id.clone(),
            source_token_account_info.clone(),
            dist_token_account_info.clone(),
            source_account_info.clone(),
        ],
    )?;

    dist.record_funded_amount(amount);

    Distribution::pack(dist, &mut dist_account_info.data.borrow_mut())?;

    Ok(())
}

fn process_set_dist_authority(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_dist_authority: &Pubkey,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let dist_account_info = next_account_info(accounts_iter)?;
    if !cmp_pubkeys(program_id, dist_account_info.owner) {
        return Err(ProgramError::IncorrectProgramId);
    }

    let dist_authority_account_info = next_account_info(accounts_iter)?;
    if !dist_authority_account_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut dist = Distribution::unpack(&dist_account_info.data.borrow())?;

    if !cmp_pubkeys(dist.dist_authority(), dist_authority_account_info.key) {
        return Err(DistError::UnauthorizedDistAuthority.into());
    }

    dist.set_dist_authority(*new_dist_authority);

    Distribution::pack(dist, &mut dist_account_info.data.borrow_mut())?;

    Ok(())
}

fn process_begin_distribution(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    num_recipients: u16,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let dist_account_info = next_account_info(accounts_iter)?;
    if !cmp_pubkeys(program_id, dist_account_info.owner) {
        return Err(ProgramError::IncorrectProgramId);
    }

    let dist_authority_account_info = next_account_info(accounts_iter)?;
    if !dist_authority_account_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut dist = Distribution::unpack(&dist_account_info.data.borrow())?;

    if !cmp_pubkeys(dist.dist_authority(), dist_authority_account_info.key) {
        return Err(DistError::UnauthorizedDistAuthority.into());
    }

    if dist.has_started() {
        return Err(DistError::DistributionAlreadyStarted.into());
    }

    dist.set_num_recipients(num_recipients);

    Distribution::pack(dist, &mut dist_account_info.data.borrow_mut())?;

    Ok(())
}

fn process_distribute(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let dist_account_info = next_account_info(accounts_iter)?;
    if !cmp_pubkeys(program_id, dist_account_info.owner) {
        return Err(ProgramError::IncorrectProgramId);
    }

    let dist_authority_account_info = next_account_info(accounts_iter)?;
    if !dist_authority_account_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let token_program_id = next_account_info(accounts_iter)?;
    spl_token::check_program_account(token_program_id.key)?;

    let dist_token_account_info = next_account_info(accounts_iter)?;

    let mut dist = Distribution::unpack(&dist_account_info.data.borrow())?;

    if !cmp_pubkeys(dist.dist_authority(), dist_authority_account_info.key) {
        return Err(DistError::UnauthorizedDistAuthority.into());
    }

    let recipient_share = dist.recipient_share();

    for recipient_token_account_info in accounts_iter {
        if dist.sent_recipients() >= dist.max_recipients() {
            return Err(DistError::TooManyRecipients.into());
        }

        if !cmp_pubkeys(recipient_token_account_info.owner, token_program_id.key) {
            return Err(ProgramError::InvalidArgument);
        }

        let distribute_tokens = spl_token::instruction::transfer(
            token_program_id.key,
            dist_token_account_info.key,
            recipient_token_account_info.key,
            dist_account_info.key,
            &[],
            recipient_share,
        )?;

        invoke_signed(
            &distribute_tokens,
            &[
                token_program_id.clone(),
                dist_token_account_info.clone(),
                recipient_token_account_info.clone(),
                dist_account_info.clone(),
            ],
            &[&[dist.data.pda_seed.seed.as_ref(),dist.data.pda_seed.project_name.as_ref() , &[dist.data.pda_seed.bump]]],
        )?;

        dist.record_sent_recipient(*recipient_token_account_info.key);
    }

    if dist.sent_recipients() >= dist.max_recipients() {}

    Distribution::pack(dist, &mut dist_account_info.data.borrow_mut())?;

    Ok(())
}

fn cmp_pubkeys(a: &Pubkey, b: &Pubkey) -> bool {
    sol_memcmp(a.as_ref(), b.as_ref(), PUBKEY_BYTES) == 0
}
