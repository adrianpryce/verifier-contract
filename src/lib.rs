pub mod str_serializers;
pub mod verified_contract;

use std::collections::HashSet;

use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{UnorderedMap, Vector};
use near_sdk::{env, log, near, require, AccountId, BorshStorageKey};
use verified_contract::comment::Comment;
use verified_contract::vote::{Vote, VoteType};
use verified_contract::VerifiedContract;

#[near(contract_state)]
pub struct SourceScan {
    owner_id: AccountId,
    contracts: UnorderedMap<AccountId, VerifiedContract>,
    comments: Vector<Comment>,
}

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    VerifiedContracts,
    Comments,
}

impl Default for SourceScan {
    fn default() -> Self {
        panic!("SourceScan should be initialized before usage")
    }
}

#[near]
impl SourceScan {
    #[init]
    pub fn new() -> Self {
        assert!(!env::state_exists(), "Already initialized");

        Self {
            owner_id: env::predecessor_account_id(),
            contracts: UnorderedMap::new(StorageKey::VerifiedContracts),
            comments: Vector::new(StorageKey::Comments),
        }
    }

    pub fn set_owner(&mut self, owner_id: AccountId) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can call this method"
        );

        self.owner_id = owner_id;

        log!("Owner changed to {}", self.owner_id)
    }

    pub fn get_owner(&self) -> AccountId {
        return self.owner_id.clone();
    }

    pub fn set_contract(
        &mut self,
        account_id: AccountId,
        cid: String,
        code_hash: String,
        block_height: u64,
        lang: String,
    ) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can call this method"
        );

        let existing_contract: Option<VerifiedContract> = self.contracts.get(&account_id);

        self.contracts.insert(
            &account_id,
            &VerifiedContract {
                cid,
                code_hash,
                block_height,
                lang,
                votes: existing_contract
                    .as_ref()
                    .map_or(Default::default(), |c| c.votes.clone()),
                comments: existing_contract
                    .as_ref()
                    .map_or(Default::default(), |c| c.comments.clone()),
            },
        );

        let action = if existing_contract.is_some() {
            "updated"
        } else {
            "added"
        };
        log!("Contract {} {}", account_id, action);
    }

    pub fn purge_contract(&mut self, account_id: AccountId) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only owner can call this method"
        );

        self.contracts.remove(&account_id);

        log!("Contract {} removed", account_id);
    }

    pub fn get_contract(&self, account_id: AccountId) -> Option<VerifiedContract> {
        return self.contracts.get(&account_id);
    }

    pub fn search(
        &self,
        key: String,
        from_index: usize,
        limit: usize,
    ) -> (Vec<(AccountId, VerifiedContract)>, u64) {
        let mut result: Vec<(AccountId, VerifiedContract)> = Vec::new();

        for (k, v) in self.contracts.iter() {
            if k.as_str()
                .to_lowercase()
                .replace(".testnet", "")
                .replace(".near", "")
                .contains(&key.to_lowercase())
            {
                result.push((k, v));
            }
        }

        let pages: u64 = self.get_pages(result.len() as u64, limit as u64);
        let filtered: Vec<(AccountId, VerifiedContract)> =
            result.into_iter().skip(from_index).take(limit).collect();

        return (filtered, pages);
    }

    pub fn get_contracts(
        &self,
        from_index: usize,
        limit: usize,
    ) -> (Vec<(AccountId, VerifiedContract)>, u64) {
        let filtered: Vec<(AccountId, VerifiedContract)> =
            self.contracts.iter().skip(from_index).take(limit).collect();

        let pages: u64 = self.get_pages(self.contracts.len(), limit as u64);

        return (filtered, pages);
    }

    pub fn vote_contract(&mut self, account_id: AccountId, is_upvote: bool) {
        let mut contract: VerifiedContract = self
            .contracts
            .get(&account_id)
            .unwrap_or_else(|| panic!("Contract {} not found", account_id))
            .into();

        self.update_or_insert_vote(&mut contract.votes, is_upvote);

        self.contracts.insert(&account_id, &contract);
        log!("Vote updated for contract {}", account_id);
    }

    pub fn add_comment(&mut self, account_id: AccountId, content: String) {
        let mut contract: VerifiedContract = self
            .contracts
            .get(&account_id)
            .unwrap_or_else(|| panic!("Contract {} not found", account_id))
            .into();

        let author_id = env::predecessor_account_id();
        let current_timestamp = env::block_timestamp();

        let new_comment = Comment {
            id: self.comments.len() as u64,
            author_id: author_id.clone(),
            timestamp: current_timestamp,
            content: content,
            votes: Default::default(),
        };

        contract.comments.push(new_comment.id);
        self.comments.push(&new_comment);
        self.contracts.insert(&account_id, &contract);
        log!("Comment added for contract {}", account_id);
    }

    pub fn get_comments(
        &self,
        account_id: AccountId,
        from_index: usize,
        limit: usize,
    ) -> (Vec<Comment>, u64) {
        let contract: VerifiedContract = self
            .contracts
            .get(&account_id)
            .unwrap_or_else(|| panic!("Contract {} not found", account_id))
            .into();

        let mut comments: Vec<Comment> = Vec::new();

        for comment_id in contract.comments {
            comments.push(self.comments.get(comment_id).unwrap());
        }

        // sort by upvotes
        comments.sort_by(|a, b| {
            let a = a
                .votes
                .iter()
                .filter(|&v| matches!(v.vote_type, VoteType::Upvote))
                .count();
            let b = b
                .votes
                .iter()
                .filter(|&v| matches!(v.vote_type, VoteType::Upvote))
                .count();
            b.cmp(&a)
        });

        let pages: u64 = self.get_pages(comments.len() as u64, limit as u64);
        let filtered: Vec<Comment> = comments.into_iter().skip(from_index).take(limit).collect();

        return (filtered, pages);
    }

    pub fn vote_comment(&mut self, comment_id: u64, is_upvote: bool) {
        require!(self.comments.get(comment_id).is_some(), "Comment not found");

        let mut comment: Comment = self
            .comments
            .get(comment_id)
            .unwrap_or_else(|| panic!("Comment {} not found", comment_id))
            .into();

        self.update_or_insert_vote(&mut comment.votes, is_upvote);

        self.comments.replace(comment_id, &comment);
        log!("Vote updated for comment {}", comment_id);
    }

    fn get_pages(&self, len: u64, limit: u64) -> u64 {
        return (len + limit - 1) / limit;
    }

    fn update_or_insert_vote(&self, votes: &mut HashSet<Vote>, is_upvote: bool) {
        let author_id = env::predecessor_account_id();
        let current_timestamp = env::block_timestamp();
        let vote_type = if is_upvote {
            VoteType::Upvote
        } else {
            VoteType::Downvote
        };

        let new_vote = Vote {
            author_id: author_id.clone(),
            timestamp: current_timestamp,
            vote_type: vote_type,
        };

        // Remove the old vote if it exists
        votes.take(&new_vote);
        // Insert the new vote
        votes.insert(new_vote);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, AccountId};

    // Helper function to set up the testing environment
    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    // Helper function to add a contract
    fn add_contract(contract: &mut SourceScan, account_id: AccountId) {
        contract.set_contract(
            account_id,
            "cid".to_string(),
            "code_hash".to_string(),
            0,
            "lang".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "SourceScan should be initialized before usage")]
    fn default_constructor() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SourceScan::default();
        contract.get_owner(); // This should panic
    }

    #[test]
    fn init_constructor() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SourceScan::new();
        assert_eq!(contract.owner_id, accounts(0));
    }

    #[test]
    fn set_and_get_owner() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn set_owner_unauthorized() {
        let context = get_context(accounts(1));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        contract.set_owner(accounts(2));
        contract.set_owner(accounts(3)); // This should panic
    }

    #[test]
    fn set_and_get_contract() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = SourceScan::new();

        add_contract(&mut contract, accounts(1));

        let contract_data = contract.get_contract(accounts(1)).unwrap();
        assert_eq!(contract_data.cid, "cid");
        assert_eq!(contract_data.code_hash, "code_hash");
        assert_eq!(contract_data.lang, "lang");
    }

    #[test]
    fn purge_and_verify_contract() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = SourceScan::new();

        add_contract(&mut contract, accounts(1));

        contract.purge_contract(accounts(1));

        assert!(contract.get_contract(accounts(1)).is_none());
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn purge_contract_unauthorized() {
        let context = get_context(accounts(1));
        testing_env!(context.build());
        let mut contract = SourceScan::new();
        contract.set_owner(accounts(2));
        contract.purge_contract(accounts(2));
    }

    #[test]
    fn list_and_verify_contracts() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = SourceScan::new();

        for i in 1..4 {
            add_contract(&mut contract, accounts(i));
        }

        let (contracts, total_pages) = contract.get_contracts(0, 2);

        assert_eq!(contracts.len(), 2);
        assert_eq!(total_pages, 2);
    }

    #[test]
    fn search_contracts() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = SourceScan::new();

        // Setup: Add contracts with varying account_ids using the helper function
        add_contract(&mut contract, "account1.testnet".parse().unwrap());
        add_contract(&mut contract, "account2.testnet".parse().unwrap());

        // Action: Search for contracts
        let (search_results, _) = contract.search("account1".to_string(), 0, 10);

        // Verification: Check if the correct contract is retrieved
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].0.to_string(), "account1.testnet");
    }

    #[test]
    fn test_vote_functionality() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        add_contract(&mut contract, accounts(1));

        // Upvote the contract
        contract.vote_contract(accounts(1), true);

        let contract_data = contract.get_contract(accounts(1)).unwrap();
        assert_eq!(contract_data.votes.len(), 1);
        assert!(matches!(
            contract_data.votes.iter().next().unwrap().vote_type,
            VoteType::Upvote
        ));

        // Change to downvote
        contract.vote_contract(accounts(1), false);

        let contract_data = contract.get_contract(accounts(1)).unwrap();
        assert!(matches!(
            contract_data.votes.iter().next().unwrap().vote_type,
            VoteType::Downvote
        ));
    }

    #[test]
    fn test_add_comment() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        add_contract(&mut contract, accounts(1));

        contract.add_comment(accounts(1), "Sample comment".to_string());

        // Adjusted to include from_index and limit
        let (comments, _) = contract.get_comments(accounts(1), 0, 10);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, "Sample comment");
    }

    #[test]
    fn test_get_comments() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        add_contract(&mut contract, accounts(1));

        contract.add_comment(accounts(1), "First comment".to_string());
        contract.add_comment(accounts(1), "Second comment".to_string());

        // Adjusted to include from_index and limit
        let (comments, pages) = contract.get_comments(accounts(1), 0, 10);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].content, "First comment");
        assert_eq!(comments[1].content, "Second comment");
        assert_eq!(pages, 1);
    }

    #[test]
    fn test_vote_comment_upvote() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        add_contract(&mut contract, accounts(1));

        contract.add_comment(accounts(1), "Another Test Comment".to_string());

        let comment_id = 0; // Assuming this is the first comment added, its id will be 0

        contract.vote_comment(comment_id, true); // Upvote the comment

        let comment = contract.comments.get(comment_id).unwrap();
        assert_eq!(comment.votes.len(), 1);
        assert!(matches!(
            comment.votes.iter().next().unwrap().vote_type,
            VoteType::Upvote
        ));
    }

    #[test]
    fn test_vote_comment_downvote() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SourceScan::new();
        add_contract(&mut contract, accounts(1));

        contract.add_comment(accounts(1), "Another Test Comment".to_string());

        let comment_id = 0; // Assuming this is the first comment added, its id will be 0

        contract.vote_comment(comment_id, false);
        let comment = contract.comments.get(comment_id).unwrap();
        assert_eq!(comment.votes.len(), 1);
        assert!(matches!(
            comment.votes.iter().next().unwrap().vote_type,
            VoteType::Downvote
        ));
    }
}
