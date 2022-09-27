//! Instruction types

use std::mem::size_of;

use solana_program::{
    instruction::{AccountMeta, Instruction},
    msg,
    program_error::ProgramError,
    pubkey::{Pubkey, PUBKEY_BYTES},
    system_program,
};

use crate::error::DistError;

/// Instructions supported by the token program.
#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub enum DistInstruction {
    /// Index: 0
    ///
    /// Initializes a new distribution.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[signer]` The funding account.
    ///   1. `[]` The system program ID.
    ///   2. `[]` Token program ID.
    ///   3. `[]` Token address.
    ///   4. `[writable]` The distribution account.
    ///   4. `[writable]` The project name.
    ///
    InitializeDistribution {
        seed: Pubkey,
        project_name:Pubkey,
        seed_bump: u8,
        // The maximum number of recipients for the distribution.
        // Affects space allocation which is necessary to avoid
        // double distribution.
        max_recipients: u16,
        // The dist authority who is able to perform distribution.
        dist_authority: Pubkey,
    },

    /// Index: 1
    ///
    /// Initializes a new distribution.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[signer]` The token source account.
    ///   1. `[writable]` The token source account's token account.
    ///   2. `[writable]` The distribution account.
    ///   3. `[writable]` The distribution account's token account.
    ///   4. `[]` Token program ID.
    ///
    FundDistribution { amount: u64 },

    /// Index: 2
    ///
    /// Sets a new owner for the distribution.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Distribution account.
    ///   1. `[signer]` The current dist authority.
    ///
    SetDistAuthority { new_dist_authority: Pubkey },

    /// Index: 3
    ///
    /// Begins distribution, locking the final number of recipients and
    /// the amount per recipient.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Distribution account.
    ///   1. `[signer]` The dist authority.
    ///
    BeginDistribution { num_recipients: u16 },

    /// Index: 4
    ///
    /// Performs distribution to the provided recipient accounts.
    ///
    /// This instruction is called as many times as necessary to reach the
    /// total number of recipients. Only one distribution per recipient is
    /// allowed.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   0. `[writable]` Distribution account.
    ///   1. `[signer]` The dist authority.
    ///   2. `[]` Token program ID.
    ///   3. `[writable]` Distribution token account.
    ///   4. ..4+M `[writable]` M recipients.
    ///
    Distribute,
}

impl DistInstruction {
    /// Unpacks a byte buffer into a [DistInstruction](enum.DistInstruction.html).
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        use DistError::InvalidInstruction;

        let (&tag, rest) = input.split_first().ok_or(InvalidInstruction)?;

        Ok(match tag {
            0 => {
                let (seed, rest) = Self::unpack_pubkey(rest)?;
                // strip the project name pubkey from 
                let (project_name, rest) = Self::unpack_pubkey(rest)?;
                let (seed_bump, rest) = rest.split_at(1);
                let seed_bump = seed_bump[0];

                let (max_recipients, rest) = rest.split_at(2);

                let max_recipients = max_recipients
                    .try_into()
                    .ok()
                    .map(u16::from_le_bytes)
                    .ok_or(InvalidInstruction)?;

                let (dist_authority, _rest) = Self::unpack_pubkey(rest)?;

                Self::InitializeDistribution {
                    seed,
                    project_name,
                    seed_bump,
                    max_recipients,
                    dist_authority,
                }
            }
            1 => {
                let (amount, _rest) = rest.split_at(8);

                let amount = amount
                    .try_into()
                    .ok()
                    .map(u64::from_le_bytes)
                    .ok_or(InvalidInstruction)?;

                Self::FundDistribution { amount }
            }
            2 => {
                let (new_dist_authority, _rest) = Self::unpack_pubkey(rest)?;

                Self::SetDistAuthority { new_dist_authority }
            }
            3 => {
                let (num_recipients, _rest) = rest.split_at(2);

                let num_recipients = num_recipients
                    .try_into()
                    .ok()
                    .map(u16::from_le_bytes)
                    .ok_or(InvalidInstruction)?;

                Self::BeginDistribution { num_recipients }
            }
            4 => Self::Distribute,
            _ => {
                return Err(InvalidInstruction.into());
            }
        })
    }

    fn unpack_pubkey(input: &[u8]) -> Result<(Pubkey, &[u8]), ProgramError> {
        if input.len() >= PUBKEY_BYTES {
            let (key, rest) = input.split_at(PUBKEY_BYTES);
            let pk = Pubkey::new(key);
            Ok((pk, rest))
        } else {
            Err(DistError::InvalidInstruction.into())
        }
    }

    /// Packs a [DistInstruction](enum.DistInstruction.html) into a byte buffer.
    pub fn pack(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size_of::<Self>());
        match self {
            &Self::InitializeDistribution {
                ref seed,
                ref project_name,
                seed_bump,
                max_recipients,
                ref dist_authority,
            } => {
                buf.push(0);
                buf.extend_from_slice(seed.as_ref());
                buf.extend_from_slice(project_name.as_ref());
                buf.push(seed_bump);
                buf.extend_from_slice(&max_recipients.to_le_bytes());
                buf.extend_from_slice(dist_authority.as_ref());
            }
            &Self::FundDistribution { amount } => {
                buf.push(1);
                buf.extend_from_slice(&amount.to_le_bytes());
            }
            &Self::SetDistAuthority {
                ref new_dist_authority,
            } => {
                buf.push(2);
                buf.extend_from_slice(new_dist_authority.as_ref());
            }
            &Self::BeginDistribution { num_recipients } => {
                buf.push(3);
                buf.extend_from_slice(&num_recipients.to_le_bytes());
            }
            &Self::Distribute => {
                buf.push(4);
            }
        }
        buf
    }
}

pub fn init_distribution(
    program_id: &Pubkey,
    token: &Pubkey,
    dist_account: &Pubkey,
    fee_payer_account: &Pubkey,
    seed: &Pubkey,
    project_name: &Pubkey,
    seed_bump: u8,
    max_recipients: u16,
    dist_authority_account: &Pubkey,
) -> Instruction {
    let data = DistInstruction::InitializeDistribution {
        seed: *seed,
        project_name: *project_name,
        seed_bump,
        max_recipients,
        dist_authority: *dist_authority_account,
    }
    .pack();

    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(*fee_payer_account, true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(*token, false),
            AccountMeta::new(*dist_account, false),
        ],
        data,
    }
}

pub fn fund_distribution(
    program_id: &Pubkey,
    source_account: &Pubkey,
    source_token_account: &Pubkey,
    dist_account: &Pubkey,
    dist_token_account: &Pubkey,
    amount: u64,
) -> Instruction {
    let data = DistInstruction::FundDistribution { amount }.pack();

    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new_readonly(*source_account, true),
            AccountMeta::new(*source_token_account, false),
            AccountMeta::new(*dist_account, false),
            AccountMeta::new(*dist_token_account, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data,
    }
}

pub fn set_dist_authority(
    program_id: &Pubkey,
    dist_account: &Pubkey,
    dist_authority: &Pubkey,
    new_dist_authority: &Pubkey,
) -> Instruction {
    let data = DistInstruction::SetDistAuthority {
        new_dist_authority: *new_dist_authority,
    }
    .pack();

    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(*dist_account, false),
            AccountMeta::new_readonly(*dist_authority, true),
        ],
        data,
    }
}

pub fn begin_distribution(
    program_id: &Pubkey,
    dist_account: &Pubkey,
    dist_authority: &Pubkey,
    num_recipients: u16,
) -> Instruction {
    let data = DistInstruction::BeginDistribution { num_recipients }.pack();

    Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(*dist_account, false),
            AccountMeta::new_readonly(*dist_authority, true),
        ],
        data,
    }
}

pub fn distribute(
    program_id: &Pubkey,
    dist_account: &Pubkey,
    dist_authority: &Pubkey,
    dist_token_account: &Pubkey,
    recipient_token_accounts: &[&Pubkey],
) -> Instruction {
    let data = DistInstruction::Distribute.pack();

    let mut accounts = Vec::with_capacity(4 + recipient_token_accounts.len());
    accounts.push(AccountMeta::new(*dist_account, false));
    accounts.push(AccountMeta::new_readonly(*dist_authority, true));
    accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
    accounts.push(AccountMeta::new(*dist_token_account, false));
    for recipient in recipient_token_accounts.iter() {
        accounts.push(AccountMeta::new(**recipient, false));
    }

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}
