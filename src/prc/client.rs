use std::error::Error;

use curve25519_dalek::Scalar;
use ed25519_dalek::{Signature, VerifyingKey};
use rand_core::CryptoRngCore;

use crate::{
    cor_rnd::{ArithValueT, BooleanValue},
    prc::{
        commitment::{Commitment, Opening}, config::{DBConfig, SystemConfig}, util::{secret_share_arith, secret_share_boolean}
    },
};

pub struct PRCClient {
    party_num: usize,
    db_conf: DBConfig,
    sys_conf: SystemConfig,
}

#[derive(Debug, Clone)]
pub struct PRCClientSt{
    idx: Scalar,
    crnd: Scalar,
}
impl PRCClientSt  {
    pub fn get_commitment(&self, rec_value: u32) -> Commitment{
        let op = Opening::new(self.crnd , self.idx, Scalar::from(rec_value));
        Commitment::commit(&op)
    }
}

#[derive(Debug, Clone)]
pub struct PRCQuery{
    pub l1_bss: BooleanValue,
    pub l2_bss: BooleanValue,
    pub rnd_ass: ArithValueT,
}

#[derive(Debug, Clone)]
pub struct PRCToken{
    pub commitment: Option<Commitment>,
    pub sig: Signature,
}
impl PRCToken{
    /// checks if the token is a valid for record value:rec_val and if valid sets the commitment
    pub fn check_for_rec_val(&mut self, st: &PRCClientSt, rec_val: u32, server_pk: &VerifyingKey) -> bool{
        let commit = st.get_commitment(rec_val);
        if let Ok(()) =  server_pk.verify_strict(&commit.serialize(), &self.sig){
            self.commitment = Some(commit);
            return true
        }
        return false
    }

    pub fn verify(&self, server_pk: &VerifyingKey) -> Result<(), Box<dyn Error>>{
        let commitment = self.commitment.as_ref().ok_or("missing value")?;
        server_pk.verify_strict(&commitment.serialize(), &self.sig)?;
        Ok(())
    } 
}

impl PRCClient {
    /// Creates a new RPCClient with the given configuration.
    pub fn new(party_num: usize, db_conf: DBConfig, sys_conf: SystemConfig) -> Self {
        PRCClient {
            party_num,
            db_conf,
            sys_conf,
        }
    }

    fn get_logdim(&self) -> (u8, u8) {
        let log_l1 = self.db_conf.l1_dim().trailing_zeros() as u8;
        let log_l2 = self.db_conf.l2_dim().trailing_zeros() as u8;
        (log_l1, log_l2)
    }

    pub fn get_dims(&self) -> (usize, usize) {
        (self.db_conf.l1_dim(), self.db_conf.l2_dim())
    }

    /// Converts a index to a 2D index (l1, l2).
    ///
    /// # Panics
    /// Panics if the index is larger than total size.
    pub fn idx_to_2dim(&self, idx: usize) -> (usize, usize) {
        assert!(idx < self.db_conf.get_max_size(), "Index out of bounds");
        let l2 = idx / self.db_conf.l1_dim();
        let l1 = idx % self.db_conf.l1_dim();
        (l1, l2)
    }

    pub fn total_db_size(&self) -> usize {
        self.db_conf.get_max_size()
    }


    /// Create a secret-shared query for the given index.
    pub fn _query_index<R: CryptoRngCore>(&self, idx: usize, rng: &mut R) -> (Vec<BooleanValue>, Vec<BooleanValue>) {
        let (dim1, dim2) = self.get_logdim();
        let (l1, l2) = self.idx_to_2dim(idx);
        let l1 = BooleanValue::new(dim1, l1 as u32);
        let l2 = BooleanValue::new(dim2, l2 as u32);
        (
            secret_share_boolean(l1, self.party_num, rng),
            secret_share_boolean(l2, self.party_num, rng),
        )
    }
    /// Chooses the randomness used in commitment and secret shares it among servers
    pub fn _query_commit_rnd<R: CryptoRngCore>(&self, rng: &mut R) -> (ArithValueT, Vec<ArithValueT>) {
        let rnd = ArithValueT::random(rng);
        let rnd_shares= secret_share_arith(rnd, self.party_num, rng);
        (rnd, rnd_shares)
    }

    /// Create the query for all servers.
    pub fn query_all_servers<R: CryptoRngCore>(
        &self,
        idx: usize,
        rng: &mut R,
    ) ->  (PRCClientSt, Vec<PRCQuery>) {
        let (l1, l2) = self._query_index(idx, rng);
        let (rnd, rnd_shares) = self._query_commit_rnd(rng);

        let st = PRCClientSt{crnd:rnd.as_scalar(), idx: Scalar::from(idx as u32)};
        let mut queries = Vec::with_capacity(self.sys_conf.get_party_num());
        for i in 0..self.sys_conf.get_party_num(){
            queries.push(PRCQuery{
                l1_bss: l1[i],
                l2_bss: l2[i],
                rnd_ass: rnd_shares[i],
            });
        }
        (st, queries)
    }

    /// Create the share of the peer.id="server_id" from a query for the given index.
    pub fn query_single_server<R: CryptoRngCore>(
        &self,
        idx: usize,
        server_id: usize,
        rng: &mut R,
    ) -> (PRCClientSt, PRCQuery) {
        let (st, queries) = self.query_all_servers(idx,rng);
        let peer_idx = self
            .sys_conf
            .get_peer_idx(server_id)
            .expect("Could not find the peer id in client config.");
        (st, queries[peer_idx].clone())
    }

}

