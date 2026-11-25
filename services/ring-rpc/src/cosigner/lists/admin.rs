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
use crate::{
    cosign::CoSignError, error::RingRpcError, hub::Hub, policy_authority::PolicyAction,
    upstream::TransactionSource,
};

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
    fn named(kind: CoSignSubjectKind, address: Address) -> Result<Self, RingRpcError> {
        Ok(match kind {
            CoSignSubjectKind::Owner => Self::owner(
                Member::owner_tag(address.as_array()).map_err(|_| CoSignError::ListSubject)?,
            ),
            CoSignSubjectKind::Asset => Self::asset(address),
            CoSignSubjectKind::Destination => Self::destination(address),
        })
    }
}

impl<S: TransactionSource> Hub<S> {
    pub async fn edit_co_sign_list(
        &self,
        request: &EditCoSignListRequest,
    ) -> Result<CoSignListEntry, RingRpcError> {
        let ring = request.ring.0;
        let lists = self
            .co_sign_lists_of(ring, &request.auth, request.action())
            .await?;
        let entry = ListEntry {
            list: request.list,
            subject: Subject::named(request.kind, request.address.0)?,
        };
        match request.edit {
            CoSignListEdit::Set => lists.set(ring, entry).await?,
            CoSignListEdit::Remove => lists.remove(ring, entry).await?,
        }
        Ok(entry.into())
    }

    pub async fn co_sign_lists(
        &self,
        request: &GetCoSignListsRequest,
    ) -> Result<GetCoSignListsResponse, RingRpcError> {
        let ring = request.ring.0;
        let action = ReadCoSignLists {
            cursor: request.cursor.clone(),
        };
        let lists = self.co_sign_lists_of(ring, &request.auth, action).await?;
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
    async fn co_sign_lists_of<A: PolicyAuthorityAction>(
        &self,
        ring: Address,
        auth: &PolicyAuthorityAuth,
        action: A,
    ) -> Result<StoredLists, RingRpcError> {
        let shared = self.shared();
        let store = shared
            .database
            .clone()
            .ok_or(RingRpcError::CoSignerStoreMissing)?;
        let service = self.service(Some(ring))?;
        PolicyAction {
            hub: self,
            ring,
            auth,
            action,
        }
        .authorize(|authority| async move {
            service.require_served_policy_authority(authority).await?;
            Ok(())
        })
        .await?;
        Ok(StoredLists(Arc::new(store)))
    }
}
