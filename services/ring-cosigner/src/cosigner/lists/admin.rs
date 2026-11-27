//! The list methods the ring policy authority signs.

use std::sync::Arc;

use solana_address::Address;
use zolana_ring_client::rpc::{
    CoSignListEdit, CoSignListEntry, CoSignSubjectKind, EditCoSignListRequest,
    GetCoSignListsRequest, GetCoSignListsResponse, PolicyAuthorityAction, PolicyAuthorityAuth,
    ReadCoSignLists,
};
use zolana_ring_policy::Member;

use super::{
    decision::{ListEntry, Subject},
    store::StoredLists,
};
use ring_service_core::upstream::TransactionSource;

use ring_audit::{policy_action::PolicyAction, shared::Shared};

use crate::{cosign::CoSignError, error::RingCosignerError};

const LIST_PAGE: u64 = 256;

impl From<ListEntry> for CoSignListEntry {
    fn from(entry: ListEntry) -> Self {
        Self {
            list: entry.list,
            kind: entry.subject.kind,
            subject: entry.subject.id.into(),
        }
    }
}

impl From<&CoSignListEntry> for ListEntry {
    fn from(entry: &CoSignListEntry) -> Self {
        Self {
            list: entry.list,
            subject: Subject {
                kind: entry.kind,
                id: entry.subject.0,
            },
        }
    }
}

impl Subject {
    /// An owner is listed by the identity the frame names it with.
    fn named(kind: CoSignSubjectKind, address: Address) -> Result<Self, RingCosignerError> {
        Ok(match kind {
            CoSignSubjectKind::Owner => Self::owner(
                Member::owner_tag(address.as_array()).map_err(|_| CoSignError::ListSubject)?,
            ),
            CoSignSubjectKind::Asset => Self::asset(address),
            CoSignSubjectKind::Destination => Self::destination(address),
        })
    }
}

pub async fn edit_co_sign_list<S: TransactionSource>(
    shared: &Arc<Shared<S>>,
    request: &EditCoSignListRequest,
) -> Result<CoSignListEntry, RingCosignerError> {
    let ring = request.ring.0;
    let entry = ListEntry {
        list: request.list,
        subject: Subject::named(request.kind, request.address.0)?,
    };
    let lists = co_sign_lists_of(shared, ring, &request.auth, request.action()).await?;
    match request.edit {
        CoSignListEdit::Set => lists.set(ring, entry).await?,
        CoSignListEdit::Remove => lists.remove(ring, entry).await?,
    }
    Ok(entry.into())
}

pub async fn co_sign_lists<S: TransactionSource>(
    shared: &Arc<Shared<S>>,
    request: &GetCoSignListsRequest,
) -> Result<GetCoSignListsResponse, RingCosignerError> {
    let ring = request.ring.0;
    let action = ReadCoSignLists {
        cursor: request.cursor.clone(),
    };
    let lists = co_sign_lists_of(shared, ring, &request.auth, action).await?;
    let after = request.cursor.as_ref().map(ListEntry::from);
    let entries: Vec<CoSignListEntry> = lists
        .entries(ring, after, LIST_PAGE)
        .await?
        .into_iter()
        .map(CoSignListEntry::from)
        .collect();
    let full = u64::try_from(entries.len()).is_ok_and(|read| read == LIST_PAGE);
    Ok(GetCoSignListsResponse {
        next_cursor: entries.last().filter(|_| full).cloned(),
        entries,
    })
}

/// The ring policy authority alone reads and edits the ring's lists.
async fn co_sign_lists_of<S: TransactionSource, A: PolicyAuthorityAction>(
    shared: &Arc<Shared<S>>,
    ring: Address,
    auth: &PolicyAuthorityAuth,
    action: A,
) -> Result<StoredLists, RingCosignerError> {
    let store = shared
        .database
        .durable()
        .cloned()
        .ok_or(RingCosignerError::CoSignerStoreMissing)?;
    let service = shared.service(Some(ring))?;
    PolicyAction {
        shared,
        ring,
        auth,
        action,
    }
    .authorize::<_, _, RingCosignerError, _, _>(|authority| async move {
        service.require_served_policy_authority(authority).await
    })
    .await?;
    Ok(StoredLists(Arc::new(store)))
}
