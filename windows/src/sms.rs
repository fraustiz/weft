//! What this computer knows of a phone's SMS and contacts (`docs/features.md`
//! section 7), and the rules that keep it tidy. All of it lives in memory:
//! nothing about messages or contacts is ever written to disk or logged.

use std::collections::{BTreeMap, BTreeSet};

use crate::packets::{Contact, Conversation, SmsMessage};

/// The most threads the phone sends.
pub const MAX_CONVERSATIONS: usize = 100;
/// How many messages one request asks for.
pub const PAGE: u32 = 50;
/// The most messages kept per thread; the oldest go first.
pub const MAX_THREAD: usize = 2000;
/// The most contacts kept.
pub const MAX_CONTACTS: usize = 5000;
/// The most messages sent from here kept with their state.
pub const OUTBOX_KEPT: usize = 32;
/// The longest text sent at once, in characters.
pub const MAX_TEXT: usize = 2000;

/// Where a message sent from here stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendState {
    /// Given to the phone, no answer yet.
    Pending,
    Sent,
    /// The phone's reason, when it gave one.
    Failed(Option<String>),
}

/// A message sent from this computer, shown until the thread has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingSms {
    pub request_id: String,
    pub addresses: Vec<String>,
    pub text: String,
    /// The thread it was sent in, `None` for a new conversation.
    pub thread: Option<i64>,
    pub state: SendState,
    /// When it was sent, in ms since the epoch.
    pub at: i64,
}

/// A phone's messages and contacts, as far as this computer loaded them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sms {
    /// The threads, newest first; `None` until the phone sent them.
    pub conversations: Option<Vec<Conversation>>,
    /// The messages loaded of each thread, oldest first.
    pub threads: BTreeMap<i64, Vec<SmsMessage>>,
    /// The threads with nothing older left to load.
    pub complete: BTreeSet<i64>,
    pub outbox: Vec<OutgoingSms>,
    pub contacts: Vec<Contact>,
}

impl Sms {
    pub fn conversation(&self, thread: i64) -> Option<&Conversation> {
        self.conversations
            .as_ref()?
            .iter()
            .find(|conversation| conversation.thread == thread)
    }

    /// The thread with exactly these people, if the phone listed it.
    pub fn conversation_with(&self, addresses: &[String]) -> Option<&Conversation> {
        self.conversations
            .as_ref()?
            .iter()
            .find(|conversation| same_people(&conversation.addresses, addresses))
    }

    /// The name of whoever has `address`: from the thread's own names,
    /// else from the contacts.
    pub fn name_of(&self, thread: i64, address: &str) -> Option<String> {
        let from_thread = self.conversation(thread).and_then(|conversation| {
            let index = conversation
                .addresses
                .iter()
                .position(|known| same_address(known, address))?;
            conversation.names.get(index).cloned().flatten()
        });
        from_thread
            .or_else(|| contact_name(&self.contacts, address).map(str::to_owned))
            .filter(|name| !name.trim().is_empty())
    }

    /// Adds `messages` to `thread`, if it is loaded or `load` says so.
    /// Returns how many it did not have.
    pub fn add_messages(&mut self, thread: i64, messages: Vec<SmsMessage>, load: bool) -> usize {
        if !load && !self.threads.contains_key(&thread) {
            return 0;
        }
        let list = self.threads.entry(thread).or_default();
        let added = merge(list, messages);
        let threads = &self.threads;
        self.outbox.retain(|sent| !shown(sent, thread, threads));
        added
    }

    /// Forgets the oldest finished messages sent from here past
    /// [`OUTBOX_KEPT`].
    pub fn trim_outbox(&mut self) {
        let mut extra = self.outbox.len().saturating_sub(OUTBOX_KEPT);
        self.outbox.retain(|sent| {
            if extra > 0 && sent.state != SendState::Pending {
                extra -= 1;
                return false;
            }
            true
        });
    }
}

/// Whether a message sent from here now shows in its thread: sent, and
/// the thread has an outgoing message with its text from about then. The
/// two clocks may differ, so "about" is five minutes.
fn shown(sent: &OutgoingSms, thread: i64, threads: &BTreeMap<i64, Vec<SmsMessage>>) -> bool {
    const SLACK: i64 = 5 * 60 * 1000;
    if sent.state != SendState::Sent || sent.thread.is_some_and(|own| own != thread) {
        return false;
    }
    threads.get(&thread).is_some_and(|messages| {
        messages.iter().any(|message| {
            message.outgoing && message.body == sent.text && message.time >= sent.at - SLACK
        })
    })
}

/// Adds `new` to `list` (by id, a known one replaced), keeps it oldest
/// first and at most [`MAX_THREAD`] long. Returns how many were new.
pub fn merge(list: &mut Vec<SmsMessage>, new: Vec<SmsMessage>) -> usize {
    let mut added = 0;
    for message in new {
        match list.iter_mut().find(|known| known.id == message.id) {
            Some(known) => *known = message,
            None => {
                list.push(message);
                added += 1;
            }
        }
    }
    list.sort_by_key(|message| (message.time, message.id));
    let extra = list.len().saturating_sub(MAX_THREAD);
    list.drain(..extra);
    added
}

/// The digits of a phone number, and a leading `+`.
pub fn digits(address: &str) -> String {
    address
        .chars()
        .enumerate()
        .filter(|(index, c)| c.is_ascii_digit() || (*index == 0 && *c == '+'))
        .map(|(_, c)| c)
        .collect()
}

/// Whether two numbers are the same line: the same digits, or the same
/// last nine when both have that many (`+33 6 12…` and `06 12…`). Names
/// and short codes compare as written.
pub fn same_address(a: &str, b: &str) -> bool {
    let (a_digits, b_digits) = (digits(a), digits(b));
    let (a_digits, b_digits) = (
        a_digits.trim_start_matches('+'),
        b_digits.trim_start_matches('+'),
    );
    if a_digits.is_empty() || b_digits.is_empty() {
        return a.trim().eq_ignore_ascii_case(b.trim());
    }
    if a_digits == b_digits {
        return true;
    }
    const TAIL: usize = 9;
    a_digits.len() >= TAIL
        && b_digits.len() >= TAIL
        && a_digits[a_digits.len() - TAIL..] == b_digits[b_digits.len() - TAIL..]
}

/// Whether two lists name the same people, in any order.
pub fn same_people(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter().all(|x| b.iter().any(|y| same_address(x, y)))
        && b.iter().all(|y| a.iter().any(|x| same_address(x, y)))
}

/// The contact with this number.
pub fn contact_name<'a>(contacts: &'a [Contact], address: &str) -> Option<&'a str> {
    contacts
        .iter()
        .find(|contact| {
            contact
                .numbers
                .iter()
                .any(|number| same_address(number, address))
        })
        .map(|contact| contact.name.as_str())
}

/// A contact's number as the recipient box offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// "Name · number".
    pub label: String,
    pub number: String,
}

/// The separator of a suggestion's name and number.
const SEPARATOR: &str = " · ";

/// The numbers whose contact name or digits hold `query`, at most `max`.
/// Nothing for a blank query.
pub fn suggestions(contacts: &[Contact], query: &str, max: usize) -> Vec<Suggestion> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let lower = query.to_lowercase();
    let query_digits = digits(query);
    let query_digits = query_digits.trim_start_matches('+');
    let mut found = Vec::new();
    for contact in contacts {
        let named = contact.name.to_lowercase().contains(&lower);
        for number in &contact.numbers {
            let dialled = !query_digits.is_empty()
                && query_digits.len() * 2 >= query.chars().filter(|c| !c.is_whitespace()).count()
                && digits(number).contains(query_digits);
            if named || dialled {
                found.push(Suggestion {
                    label: format!("{}{SEPARATOR}{number}", contact.name),
                    number: number.clone(),
                });
                if found.len() == max {
                    return found;
                }
            }
        }
    }
    found
}

/// The number a recipient box means: the number of the suggestion it
/// shows, else what was typed if it looks like a number.
pub fn recipient(contacts: &[Contact], text: &str) -> Option<String> {
    let text = text.trim();
    if let Some((name, number)) = text.rsplit_once(SEPARATOR)
        && contacts
            .iter()
            .any(|contact| contact.name == name && contact.numbers.iter().any(|n| n == number))
    {
        return Some(number.to_owned());
    }
    let looks_like_number = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || " +-.()".contains(c))
        && text.chars().any(|c| c.is_ascii_digit());
    looks_like_number.then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: i64, time: i64, body: &str, outgoing: bool) -> SmsMessage {
        SmsMessage {
            id,
            address: Some("+33 6 12 34 56 78".into()),
            body: body.into(),
            time,
            outgoing,
            status: None,
        }
    }

    #[test]
    fn messages_merge_by_id_oldest_first() {
        let mut list = vec![message(2, 20, "b", false)];
        let added = merge(
            &mut list,
            vec![message(1, 10, "a", false), message(2, 20, "b2", false)],
        );
        assert_eq!(added, 1);
        let bodies: Vec<&str> = list.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["a", "b2"]);

        let mut long: Vec<SmsMessage> = Vec::new();
        let many = (0..MAX_THREAD as i64 + 5)
            .map(|i| message(i, i, "x", false))
            .collect();
        merge(&mut long, many);
        assert_eq!(long.len(), MAX_THREAD);
        assert_eq!(long[0].id, 5);
    }

    #[test]
    fn numbers_match_across_formats() {
        assert!(same_address("+33 6 12 34 56 78", "06.12.34.56.78"));
        assert!(same_address("0612345678", "+33612345678"));
        assert!(!same_address("+33 6 12 34 56 78", "+33 6 12 34 56 79"));
        assert!(same_address("36179", "36179"));
        assert!(!same_address("3617", "36179"));
        assert!(same_address("Bank", "bank"));
        assert!(same_people(
            &["+33612345678".into(), "0700000001".into()],
            &["0700000001".into(), "06 12 34 56 78".into()]
        ));
        assert!(!same_people(&["0612345678".into()], &[]));
    }

    #[test]
    fn sent_messages_leave_the_outbox_once_the_thread_has_them() {
        let mut sms = Sms::default();
        sms.outbox.push(OutgoingSms {
            request_id: "r".into(),
            addresses: vec!["0612345678".into()],
            text: "On my way".into(),
            thread: Some(7),
            state: SendState::Pending,
            at: 1000,
        });
        // Not loaded: kept out unless asked.
        assert_eq!(
            sms.add_messages(7, vec![message(1, 900, "Hi", false)], false),
            0
        );
        assert!(sms.threads.is_empty());
        sms.add_messages(7, vec![message(1, 900, "Hi", false)], true);
        // Still pending: stays even when a twin shows.
        sms.add_messages(7, vec![message(2, 1100, "On my way", true)], false);
        assert_eq!(sms.outbox.len(), 1);
        sms.outbox[0].state = SendState::Sent;
        sms.add_messages(7, vec![message(2, 1100, "On my way", true)], false);
        assert!(sms.outbox.is_empty());
    }

    #[test]
    fn names_come_from_the_thread_then_the_contacts() {
        let mut sms = Sms {
            conversations: Some(vec![Conversation {
                thread: 7,
                addresses: vec!["0612345678".into(), "0700000001".into()],
                names: vec![Some("Ann".into()), None],
                ..Conversation::default()
            }]),
            contacts: vec![Contact {
                name: "Bob".into(),
                numbers: vec!["+33 7 00 00 00 01".into()],
            }],
            ..Sms::default()
        };
        assert_eq!(sms.name_of(7, "+33612345678").as_deref(), Some("Ann"));
        assert_eq!(sms.name_of(7, "0700000001").as_deref(), Some("Bob"));
        assert_eq!(sms.name_of(7, "0800000000"), None);
        assert_eq!(
            sms.conversation_with(&["07 00 00 00 01".into(), "+33612345678".into()])
                .map(|c| c.thread),
            Some(7)
        );
        sms.conversations = None;
        assert!(sms.conversation(7).is_none());
    }

    #[test]
    fn recipients_come_from_contacts_or_digits() {
        let contacts = vec![
            Contact {
                name: "Ann Martin".into(),
                numbers: vec!["+33 6 12 34 56 78".into(), "01 23 45 67 89".into()],
            },
            Contact {
                name: "Bob".into(),
                numbers: vec!["07 00 00 00 01".into()],
            },
        ];
        let labels = |query| {
            suggestions(&contacts, query, 10)
                .into_iter()
                .map(|s| s.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels("ann"),
            [
                "Ann Martin · +33 6 12 34 56 78",
                "Ann Martin · 01 23 45 67 89"
            ]
        );
        assert_eq!(labels("0700"), ["Bob · 07 00 00 00 01"]);
        // A name with a digit in it is not a number.
        assert!(labels("b0b").is_empty());
        assert!(labels(" ").is_empty());
        assert_eq!(suggestions(&contacts, "a", 1).len(), 1);

        assert_eq!(
            recipient(&contacts, "Bob · 07 00 00 00 01").as_deref(),
            Some("07 00 00 00 01")
        );
        assert_eq!(
            recipient(&contacts, " +33 (0)6 12 ").as_deref(),
            Some("+33 (0)6 12")
        );
        assert_eq!(recipient(&contacts, "Bob"), None);
        assert_eq!(recipient(&contacts, "Eve · 0600"), None);
    }

    #[test]
    fn the_outbox_keeps_what_is_pending() {
        let mut sms = Sms::default();
        for index in 0..OUTBOX_KEPT + 3 {
            sms.outbox.push(OutgoingSms {
                request_id: index.to_string(),
                addresses: Vec::new(),
                text: String::new(),
                thread: None,
                state: if index == 0 {
                    SendState::Pending
                } else {
                    SendState::Failed(None)
                },
                at: 0,
            });
        }
        sms.trim_outbox();
        assert_eq!(sms.outbox.len(), OUTBOX_KEPT);
        assert_eq!(sms.outbox[0].request_id, "0");
        assert_eq!(sms.outbox[1].request_id, "4");
    }
}
