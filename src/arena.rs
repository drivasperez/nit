use std::ops::{Deref, DerefMut};

use thiserror::Error;
#[derive(Debug, Error)]
pub enum ArenaError {
    #[error("Node has an existing parent")]
    ParentAssigned,
    #[error("Node already exists")]
    NodeExists,
}
#[derive(Debug, PartialEq)]
pub struct Arena<T: PartialEq> {
    arena: Vec<Node<T>>,
}

impl<T: PartialEq> Arena<T> {
    pub fn new() -> Self {
        Self { arena: vec![] }
    }

    fn node(&mut self, val: T) -> usize {
        for node in &self.arena {
            if node.val == val {
                return node.idx;
            }
        }

        let idx = self.arena.len();
        self.arena.push(Node::new(idx, val));
        idx
    }

    pub fn get_token(&self, val: &T) -> Option<Key> {
        for node in &self.arena {
            if node.val == *val {
                return Some(Key(node.idx));
            }
        }

        None
    }

    pub fn get_node_from_key(&self, key: Key) -> Option<&Node<T>> {
        self.arena.get(key.0)
    }

    pub fn get_node(&self, val: &T) -> Option<&Node<T>> {
        for node in &self.arena {
            if node.val == *val {
                return Some(&node);
            }
        }

        None
    }

    pub fn insert(&mut self, node: T) -> Key {
        let node_idx = self.node(node);

        Key(node_idx)
    }

    pub fn append(&mut self, child: Key, parent: Key) -> Result<(), ArenaError> {
        match self.arena[child.0].parent {
            Some(_) => Err(ArenaError::ParentAssigned),
            None => {
                self.arena[child.0].parent = Some(parent.0);
                self.arena[parent.0].children.push(child.0);

                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Key(usize);

#[derive(Debug, PartialEq)]
pub struct Node<T: PartialEq> {
    idx: usize,
    val: T,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl<T: PartialEq> Deref for Node<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.val
    }
}

impl<T: PartialEq> DerefMut for Node<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.val
    }
}

impl<T: PartialEq> Node<T> {
    fn new(idx: usize, val: T) -> Self {
        Self {
            idx,
            val,
            parent: None,
            children: vec![],
        }
    }
}
#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn arena_hmm() {
        let mut arena = Arena::new();
        let entry1 = arena.insert("Hey");
        let entry2 = arena.insert("Hi");
        let entry3 = arena.insert("oh no");

        arena.append(entry2, entry1).unwrap();
        arena.append(entry3, entry1).unwrap();

        assert_eq!(
            arena,
            Arena {
                arena: vec![
                    Node {
                        idx: 0,
                        val: "Hey",
                        parent: None,
                        children: vec![1, 2]
                    },
                    Node {
                        idx: 1,
                        val: "Hi",
                        parent: Some(0),
                        children: vec![]
                    },
                    Node {
                        idx: 2,
                        val: "oh no",
                        parent: Some(0),
                        children: vec![]
                    }
                ]
            }
        );
    }
}
