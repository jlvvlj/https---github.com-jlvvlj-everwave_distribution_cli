//! State transition types

use std::io::BufWriter;

use borsh::{BorshDeserialize, BorshSerialize};

use solana_program::{
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack, Sealed},
    pubkey::{Pubkey, PubkeyError, PUBKEY_BYTES},
};

const UNINITIALIZED_VERSION: u8 = 0;

const VERSION_1: u8 = 1;

const PDA_SEED_SIZE: usize = PUBKEY_BYTES + PUBKEY_BYTES+1;

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct PdaSeed {
    pub seed: Pubkey,
    pub project_name: Pubkey,
    pub bump: u8,
}

impl PdaSeed {
    pub fn new(seed: Pubkey,project_name:Pubkey, bump: u8) -> PdaSeed {
        PdaSeed { seed,project_name, bump }
    }

    pub fn create_pubkey(&self, program_id: &Pubkey) -> Result<Pubkey, PubkeyError> {
        let seeds = &[self.seed.as_ref(),self.project_name.as_ref(), &[self.bump]];
        Pubkey::create_program_address(seeds, program_id)
    }
}

const DISTRIBUTION_V1_SIZE: usize = PDA_SEED_SIZE + PUBKEY_BYTES + PUBKEY_BYTES + 2 + 2 + 8 + 2;

#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct DistributionV1 {
    pub pda_seed: PdaSeed,
    pub dist_authority: Pubkey,
    pub token: Pubkey,
    pub max_recipients: u16,
    pub num_recipients: u16,
    pub funded_amount: u64,
    pub sent_recipients: u16,
}

const DISTRIBUTION_SIZE: usize = 1 + DISTRIBUTION_V1_SIZE;

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize)]
pub struct Distribution {
    pub version: u8,
    pub data: DistributionV1,
}

impl Distribution {
    pub fn new(
        pda_seed: PdaSeed,
        dist_authority: Pubkey,
        token: Pubkey,
        max_recipients: u16,
        num_recipients: u16,
    ) -> Self {
        let mut dist = Distribution::default();
        dist.init(
            pda_seed,
            dist_authority,
            token,
            max_recipients,
            num_recipients,
        );
        dist
    }

    pub fn init(
        &mut self,
        pda_seed: PdaSeed,
        dist_authority: Pubkey,
        token: Pubkey,
        max_recipients: u16,
        num_recipients: u16,
    ) {
        self.version = VERSION_1;
        self.data.pda_seed = pda_seed;
        self.data.dist_authority = dist_authority;
        self.data.token = token;
        self.data.max_recipients = max_recipients;
        self.data.num_recipients = num_recipients;
    }
}

impl Distribution {
    pub fn pda_seed(&self) -> &PdaSeed {
        &self.data.pda_seed
    }

    pub fn token(&self) -> &Pubkey {
        &self.data.token
    }

    pub fn dist_authority(&self) -> &Pubkey {
        &self.data.dist_authority
    }

    pub fn set_dist_authority(&mut self, new_dist_authority: Pubkey) {
        self.data.dist_authority = new_dist_authority;
    }

    pub fn max_recipients(&self) -> u16 {
        self.data.max_recipients
    }

    pub fn set_max_recipients(&mut self, max_recipients: u16) {
        self.data.max_recipients = max_recipients;
    }

    pub fn num_recipients(&self) -> u16 {
        self.data.num_recipients
    }

    pub fn set_num_recipients(&mut self, num_recipients: u16) {
        self.data.num_recipients = num_recipients;
    }

    pub fn funded_amount(&self) -> u64 {
        self.data.funded_amount
    }

    pub fn recipient_share(&self) -> u64 {
        if self.data.num_recipients == 0 {
            return 0;
        }

        self.data.funded_amount / self.data.num_recipients as u64
    }

    pub fn record_funded_amount(&mut self, amount: u64) {
        self.data.funded_amount = self.data.funded_amount.checked_add(amount).unwrap();
    }

    pub fn sent_recipients(&self) -> u16 {
        self.data.sent_recipients
    }

    pub fn record_sent_recipient(&mut self, recipient: Pubkey) {
        self.data.sent_recipients += 1;
    }

    pub fn has_started(&self) -> bool {
        self.data.num_recipients > 0
    }
}

impl Default for Distribution {
    fn default() -> Self {
        return Distribution {
            version: UNINITIALIZED_VERSION,
            data: DistributionV1::default(),
        };
    }
}

impl Sealed for Distribution {}

impl IsInitialized for Distribution {
    fn is_initialized(&self) -> bool {
        self.version != UNINITIALIZED_VERSION
    }
}

impl Pack for Distribution {
    const LEN: usize = DISTRIBUTION_SIZE;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let version = src[0];
        if version == UNINITIALIZED_VERSION {
            return Ok(Distribution::default());
        }
        if version == VERSION_1 {
            return Ok(Distribution::try_from_slice(src)?);
        }
        Err(ProgramError::InvalidAccountData)
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut bw = BufWriter::with_capacity(Self::LEN, dst);
        self.serialize(&mut bw).unwrap()
    }
}
