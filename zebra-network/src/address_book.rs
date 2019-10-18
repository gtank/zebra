//! The addressbook manages information about what peers exist, when they were
//! seen, and what services they provide.

use std::{
    collections::{BTreeMap, HashMap},
    iter::Extend,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use futures::channel::mpsc;
use tokio::prelude::*;

use crate::{
    constants,
    types::{MetaAddr, PeerServices},
};

/// A database of peers, their advertised services, and information on when they
/// were last seen.
#[derive(Default, Debug)]
pub struct AddressBook {
    by_addr: HashMap<SocketAddr, (DateTime<Utc>, PeerServices)>,
    by_time: BTreeMap<DateTime<Utc>, (SocketAddr, PeerServices)>,
}

impl AddressBook {
    /// Update the address book with `event`, a [`MetaAddr`] representing
    /// observation of a peer.
    pub fn update(&mut self, event: MetaAddr) {
        use std::collections::hash_map::Entry;

        debug!(
            ?event,
            data.total = self.by_time.len(),
            data.recent = (self.by_time.len() - self.disconnected_peers().count()),
        );

        let MetaAddr {
            addr,
            services,
            last_seen,
        } = event;

        match self.by_addr.entry(addr) {
            Entry::Occupied(mut entry) => {
                let (prev_last_seen, _) = entry.get();
                // If the new timestamp event is older than the current
                // one, discard it.  This is irrelevant for the timestamp
                // collector but is important for combining address
                // information from different peers.
                if *prev_last_seen > last_seen {
                    return;
                }
                self.by_time
                    .remove(prev_last_seen)
                    .expect("cannot have by_addr entry without by_time entry");
                entry.insert((last_seen, services));
                self.by_time.insert(last_seen, (addr, services));
            }
            Entry::Vacant(entry) => {
                entry.insert((last_seen, services));
                self.by_time.insert(last_seen, (addr, services));
            }
        }
    }

    /// Return an iterator over all peers, ordered from most recently seen to
    /// least recently seen.
    pub fn peers<'a>(&'a self) -> impl Iterator<Item = MetaAddr> + 'a {
        self.by_time.iter().rev().map(from_by_time_kv)
    }

    /// Return an iterator over peers known to be disconnected, ordered from most
    /// recently seen to least recently seen.
    pub fn disconnected_peers<'a>(&'a self) -> impl Iterator<Item = MetaAddr> + 'a {
        use chrono::Duration as CD;
        use std::ops::Bound::{Excluded, Unbounded};

        // LIVE_PEER_DURATION represents the time interval in which we are
        // guaranteed to receive at least one message from a peer or close the
        // connection. Therefore, if the last-seen timestamp is older than
        // LIVE_PEER_DURATION ago, we know we must have disconnected from it.
        let cutoff = Utc::now() - CD::from_std(constants::LIVE_PEER_DURATION).unwrap();

        self.by_time
            .range((Unbounded, Excluded(cutoff)))
            .rev()
            .map(from_by_time_kv)
    }

    /// Returns an iterator that drains entries from the address book, removing
    /// them in order from most recent to least recent.
    pub fn drain_recent<'a>(&'a mut self) -> impl Iterator<Item = MetaAddr> + 'a {
        Drain { book: self }
    }
}

// Helper impl to convert by_time Iterator Items back to MetaAddrs
// This could easily be a From impl, but trait impls are public, and this shouldn't be.
fn from_by_time_kv(by_time_kv: (&DateTime<Utc>, &(SocketAddr, PeerServices))) -> MetaAddr {
    let (last_seen, (addr, services)) = by_time_kv;
    MetaAddr {
        last_seen: last_seen.clone(),
        addr: addr.clone(),
        services: services.clone(),
    }
}

impl Extend<MetaAddr> for AddressBook {
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = MetaAddr>,
    {
        for meta in iter.into_iter() {
            self.update(meta);
        }
    }
}

struct Drain<'a> {
    book: &'a mut AddressBook,
}

impl<'a> Iterator for Drain<'a> {
    type Item = MetaAddr;

    fn next(&mut self) -> Option<Self::Item> {
        let most_recent = self.book.by_time.keys().rev().next()?.clone();
        let (addr, services) = self
            .book
            .by_time
            .remove(&most_recent)
            .expect("key from keys() must be present in btreemap");
        self.book.by_addr.remove(&addr);
        Some(MetaAddr {
            addr,
            services,
            last_seen: most_recent,
        })
    }
}