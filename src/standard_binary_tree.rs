//! This module contains the [StandardMerkleTree], an implementation of the standard Merkle Tree data structure.
//!
//! Check out [StandardMerkleTree](https://github.com/OpenZeppelin/merkle-tree) for more details.
//!
//! # Examples
//!
//! ```rust
//! use alloy_merkle_tree::standard_binary_tree::StandardMerkleTree;
//! use alloy::dyn_abi::DynSolValue;
//!
//! let num_leaves = 1000;
//! let mut leaves = Vec::new();
//! for i in 0..num_leaves {
//!     leaves.push(DynSolValue::String(i.to_string()));
//! }
//! let tree = StandardMerkleTree::of(&leaves);
//!
//! for leaf in leaves.iter() {
//!     let proof = tree.get_proof(leaf).unwrap();
//!     let is_valid = tree.verify_proof(leaf, proof);
//!     assert!(is_valid);
//! }
//! ```
//!
use core::panic;

use crate::alloc::string::ToString;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloy::dyn_abi::DynSolValue;
use alloy::primitives::{keccak256, Keccak256, B256};

use hashbrown::HashMap;

/// The error type for the [StandardMerkleTree].
#[derive(Debug)]
pub enum MerkleTreeError {
    /// The specified leaf was not found in the tree.
    LeafNotFound,
    /// An invalid check occurred during tree operations.
    InvalidCheck,
    /// The root node does not have any siblings.
    RootHaveNoSiblings,
    /// The leaf type is not supported by the tree.
    NotSupportedType,
}

/// Represents a standard Merkle tree with methods for proof generation and verification.
#[derive(Debug)]
pub struct StandardMerkleTree {
    /// The internal representation of the tree as a flat vector.
    tree: Vec<B256>,
    /// A mapping from serialized leaf values to their indices in the tree.
    tree_values: HashMap<String, usize>,
}

impl Default for StandardMerkleTree {
    /// Creates a new, empty `StandardMerkleTree`.
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new())
    }
}

impl StandardMerkleTree {
    /// Creates a new [`StandardMerkleTree`] with the given tree nodes and values.
    pub fn new(tree: Vec<B256>, values: Vec<(&DynSolValue, usize)>) -> Self {
        let mut tree_values = HashMap::new();
        for (tree_key, tree_value) in values.into_iter() {
            let tree_key_str = Self::check_valid_value_type(tree_key);
            tree_values.insert(tree_key_str, tree_value);
        }
        Self { tree, tree_values }
    }

    /// Constructs a [`StandardMerkleTree`] from a slice of dynamic Solidity values.
    pub fn of(values: &[DynSolValue]) -> Self {
        // Hash each value and associate it with its index and leaf hash.
        let hashed_values: Vec<(&DynSolValue, usize, B256)> = values
            .iter()
            .enumerate()
            .map(|(i, value)| (value, i, standard_leaf_hash(value)))
            .collect();

        // Collect the leaf hashes into a vector.
        let hashed_values_hash = hashed_values
            .iter()
            .map(|(_, _, hash)| *hash)
            .collect::<Vec<B256>>();

        // Build the Merkle tree from the leaf hashes.
        let tree = make_merkle_tree(hashed_values_hash);

        // Map each value to its corresponding index in the tree.
        let mut indexed_values: Vec<(&DynSolValue, usize)> =
            values.iter().map(|value| (value, 0)).collect();

        for (leaf_index, (_, value_index, _)) in hashed_values.iter().enumerate() {
            indexed_values[*value_index].1 = tree.len() - leaf_index - 1;
        }

        Self::new(tree, indexed_values)
    }

    /// Retrieves the root hash of the Merkle tree.
    pub fn root(&self) -> B256 {
        self.tree[0]
    }

    /// Generates a Merkle proof for a given leaf value.
    pub fn get_proof(&self, value: &DynSolValue) -> Result<Vec<B256>, MerkleTreeError> {
        let tree_key = Self::check_valid_value_type(value);

        let tree_index = self
            .tree_values
            .get(&tree_key)
            .ok_or(MerkleTreeError::LeafNotFound)?;

        make_proof(&self.tree, *tree_index)
    }

    /// Computes the hash of a leaf node.
    fn get_leaf_hash(&self, leaf: &DynSolValue) -> B256 {
        standard_leaf_hash(leaf)
    }

    /// Verifies a Merkle proof for a given leaf value.
    pub fn verify_proof(&self, leaf: &DynSolValue, proof: Vec<B256>) -> bool {
        let leaf_hash = self.get_leaf_hash(leaf);
        let implied_root = process_proof(leaf_hash, proof);
        self.tree[0] == implied_root
    }

    /// Validates and serializes a [`DynSolValue`] into a [`String`].
    fn check_valid_value_type(value: &DynSolValue) -> String {
        match value {
            DynSolValue::String(inner_value) => inner_value.to_string(),
            DynSolValue::FixedBytes(inner_value, _) => inner_value.to_string(),
            _ => panic!("Not supported value type"),
        }
    }
}

/// Computes the standard leaf hash for a given value..
fn standard_leaf_hash(value: &DynSolValue) -> B256 {
    let encoded = match value {
        DynSolValue::String(inner_value) => inner_value.as_bytes(),
        DynSolValue::FixedBytes(inner_value, _) => inner_value.as_ref(),
        _ => panic!("Not supported value type for leaf"),
    };
    keccak256(keccak256(encoded))
}

/// Calculates the index of the left child for a given parent index..
fn left_child_index(index: usize) -> usize {
    2 * index + 1
}

/// Calculates the index of the right child for a given parent index.
fn right_child_index(index: usize) -> usize {
    2 * index + 2
}

/// Determines the sibling index for a given node index..
fn sibling_index(index: usize) -> Result<usize, MerkleTreeError> {
    if index == 0 {
        return Err(MerkleTreeError::RootHaveNoSiblings);
    }

    if index % 2 == 0 {
        Ok(index - 1)
    } else {
        Ok(index + 1)
    }
}

/// Calculates the parent index for a given child index.
fn parent_index(index: usize) -> usize {
    (index - 1) / 2
}

/// Checks if a given index corresponds to a node within the tree.
fn is_tree_node(tree: &[B256], index: usize) -> bool {
    index < tree.len()
}

/// Checks if a given index corresponds to an internal node (non-leaf).
fn is_internal_node(tree: &[B256], index: usize) -> bool {
    is_tree_node(tree, left_child_index(index))
}

/// Checks if a given index corresponds to a leaf node.
fn is_leaf_node(tree: &[B256], index: usize) -> bool {
    !is_internal_node(tree, index) && is_tree_node(tree, index)
}

/// Validates that a given index corresponds to a leaf node.
fn check_leaf_node(tree: &[B256], index: usize) -> Result<(), MerkleTreeError> {
    if !is_leaf_node(tree, index) {
        Err(MerkleTreeError::InvalidCheck)
    } else {
        Ok(())
    }
}

/// Constructs a Merkle tree from a vector of leaf hashes.
fn make_merkle_tree(leaves: Vec<B256>) -> Vec<B256> {
    let tree_len = 2 * leaves.len() - 1;
    let mut tree = vec![B256::default(); tree_len];
    let leaves_len = leaves.len();

    // Place leaves at the end of the tree array.
    for (i, leaf) in leaves.into_iter().enumerate() {
        tree[tree_len - 1 - i] = leaf;
    }

    // Build the tree by hashing pairs of nodes from the leaves up to the root.
    for i in (0..tree_len - leaves_len).rev() {
        let left = tree[left_child_index(i)];
        let right = tree[right_child_index(i)];

        tree[i] = hash_pair(left, right);
    }

    tree
}

/// Generates a Merkle proof for a leaf at a given index.
fn make_proof(tree: &[B256], index: usize) -> Result<Vec<B256>, MerkleTreeError> {
    check_leaf_node(tree, index)?;

    let mut proof = Vec::new();
    let mut current_index = index;
    while current_index > 0 {
        let sibling = sibling_index(current_index)?;

        if sibling < tree.len() {
            proof.push(tree[sibling]);
        }
        current_index = parent_index(current_index);
    }

    Ok(proof)
}

/// Processes a Merkle proof to compute the implied root hash.
///
/// Returns `B256` hash of the implied Merkle root.
fn process_proof(leaf: B256, proof: Vec<B256>) -> B256 {
    proof.into_iter().fold(leaf, hash_pair)
}

/// Hashes a pair of `B256` values to compute their parent hash.
fn hash_pair(left: B256, right: B256) -> B256 {
    let combined = if left <= right { left } else { right };
    let second = if left <= right { right } else { left };

    let mut hasher = Keccak256::new();
    hasher.update(combined);
    hasher.update(second);
    hasher.finalize()
}

#[cfg(test)]
mod test {
    use crate::alloc::string::ToString;
    use crate::standard_binary_tree::StandardMerkleTree;
    use alloc::vec::Vec;
    use alloy::dyn_abi::DynSolValue;
    use alloy::primitives::{hex::FromHex, FixedBytes};

    /// Tests the [`StandardMerkleTree`] with string-type leaves.
    #[test]
    fn test_tree_string_type() {
        let num_leaves = 1000;
        let mut leaves = Vec::new();
        for i in 0..num_leaves {
            leaves.push(DynSolValue::String(i.to_string()));
        }
        let tree = StandardMerkleTree::of(&leaves);

        for leaf in leaves.into_iter() {
            let proof = tree.get_proof(&leaf).unwrap();
            let is_valid = tree.verify_proof(&leaf, proof);
            assert!(is_valid);
        }
    }

    /// Tests the `StandardMerkleTree` with bytes32-type leaves.
    #[test]
    fn test_tree_bytes32_type() {
        let mut leaves = Vec::new();

        let leaf = DynSolValue::FixedBytes(
            FixedBytes::<32>::from_hex(
                "0x46296bc9cb11408bfa46c5c31a542f12242db2412ee2217b4e8add2bc1927d0b",
            )
            .unwrap(),
            32,
        );

        leaves.push(leaf);

        let tree = StandardMerkleTree::of(&leaves);

        for leaf in leaves.into_iter() {
            let proof = tree.get_proof(&leaf).unwrap();
            let is_valid = tree.verify_proof(&leaf, proof);
            assert!(is_valid);
        }
    }
}
