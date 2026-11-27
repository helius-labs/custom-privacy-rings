//! Ring RPC's co-signature for a wallet-sent transaction and its operator lists.

use serde::{Deserialize, Serialize};
use solana_address::Address;
use spr_indexer_api::{Base64String, Hash, SerializablePubkey, SerializableSignature};

use super::{
    PolicyAuthorityAction, PolicyAuthorityAttestation, PolicyAuthorityAuth, ReadTarget,
    SignedRequest,
};

pub const CO_SIGN_TRANSACTION: &str = "coSignTransaction";
pub const EDIT_CO_SIGN_LIST: &str = "editCoSignList";
pub const GET_CO_SIGN_LISTS: &str = "getCoSignLists";

/// `transaction` is the serialized v1 transaction of one ring transfer, every required signer but the co-signer signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CoSignTransactionRequest {
    pub ring: SerializablePubkey,
    pub transaction: Base64String,
    /// Set where a program calls the transfer under the message's `ApproveTransfer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_transfer: Option<ApprovedTransfer>,
}

/// The ring `Transfer` call an `ApproveTransfer` digest commits to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ApprovedTransfer {
    /// From the `TRANSFER` tag.
    pub data: Base64String,
    /// In call order.
    pub accounts: Vec<SerializablePubkey>,
}

/// `signature` signs the message of the request transaction under `signer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CoSignTransactionResponse {
    pub signer: SerializablePubkey,
    pub signature: SerializableSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoSignList {
    /// A kind with one allow entry admits only its allowed subjects.
    Allow,
    Block,
    /// Stops what a subject sends, it still receives.
    Freeze,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoSignSubjectKind {
    /// A wallet, stored as its owner identity.
    Owner,
    /// A mint.
    Asset,
    /// An account of a public withdrawal leg.
    Destination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoSignListEdit {
    Set,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditCoSignList {
    pub edit: CoSignListEdit,
    pub list: CoSignList,
    pub kind: CoSignSubjectKind,
    pub address: Address,
}

/// The page after `cursor`, the first page without one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadCoSignLists {
    pub cursor: Option<CoSignListEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EditCoSignListRequest {
    pub ring: SerializablePubkey,
    pub edit: CoSignListEdit,
    pub list: CoSignList,
    pub kind: CoSignSubjectKind,
    pub address: SerializablePubkey,
    pub auth: PolicyAuthorityAuth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetCoSignListsRequest {
    pub ring: SerializablePubkey,
    /// The last entry of the previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<CoSignListEntry>,
    pub auth: PolicyAuthorityAuth,
}

/// `subject` is the owner identity of an owner entry, the address of any other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CoSignListEntry {
    pub list: CoSignList,
    pub kind: CoSignSubjectKind,
    pub subject: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetCoSignListsResponse {
    pub entries: Vec<CoSignListEntry>,
    /// Absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<CoSignListEntry>,
}

impl CoSignList {
    pub const ALL: [Self; 3] = [Self::Allow, Self::Block, Self::Freeze];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
            Self::Freeze => "freeze",
        }
    }
}

impl CoSignSubjectKind {
    pub const ALL: [Self; 3] = [Self::Owner, Self::Asset, Self::Destination];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Asset => "asset",
            Self::Destination => "destination",
        }
    }
}

impl PolicyAuthorityAction for EditCoSignList {
    type Request = EditCoSignListRequest;

    fn line(&self) -> String {
        let edit = match self.edit {
            CoSignListEdit::Set => "set",
            CoSignListEdit::Remove => "remove",
        };
        format!(
            "{edit} co-sign {} {} {}",
            self.list.name(),
            self.kind.name(),
            self.address
        )
    }

    fn request(self, ring: Address, auth: PolicyAuthorityAuth) -> EditCoSignListRequest {
        EditCoSignListRequest {
            ring: ring.into(),
            edit: self.edit,
            list: self.list,
            kind: self.kind,
            address: self.address.into(),
            auth,
        }
    }
}

impl PolicyAuthorityAction for ReadCoSignLists {
    type Request = GetCoSignListsRequest;

    fn line(&self) -> String {
        match &self.cursor {
            None => "read co-sign lists".to_owned(),
            Some(cursor) => format!(
                "read co-sign lists after {} {} {}",
                cursor.list.name(),
                cursor.kind.name(),
                cursor.subject.to_base58()
            ),
        }
    }

    fn request(self, ring: Address, auth: PolicyAuthorityAuth) -> GetCoSignListsRequest {
        GetCoSignListsRequest {
            ring: ring.into(),
            cursor: self.cursor,
            auth,
        }
    }
}

impl EditCoSignListRequest {
    pub fn edit(
        target: ReadTarget,
        authority: Address,
        action: EditCoSignList,
    ) -> SignedRequest<PolicyAuthorityAttestation<EditCoSignList>> {
        SignedRequest::new(PolicyAuthorityAttestation {
            target,
            authority,
            action,
        })
    }

    pub const fn action(&self) -> EditCoSignList {
        EditCoSignList {
            edit: self.edit,
            list: self.list,
            kind: self.kind,
            address: self.address.0,
        }
    }
}

impl GetCoSignListsRequest {
    pub fn read(
        target: ReadTarget,
        authority: Address,
        action: ReadCoSignLists,
    ) -> SignedRequest<PolicyAuthorityAttestation<ReadCoSignLists>> {
        SignedRequest::new(PolicyAuthorityAttestation {
            target,
            authority,
            action,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALLET: Address = Address::new_from_array([7; 32]);

    /// The TypeScript client signs the same lines.
    #[test]
    fn the_authority_signs_the_edit_and_the_page_it_names() {
        let edit = |edit, list, kind| EditCoSignList {
            edit,
            list,
            kind,
            address: WALLET,
        };
        let allow = edit(
            CoSignListEdit::Set,
            CoSignList::Allow,
            CoSignSubjectKind::Owner,
        );
        let unfreeze = edit(
            CoSignListEdit::Remove,
            CoSignList::Freeze,
            CoSignSubjectKind::Destination,
        );
        assert_eq!(allow.line(), format!("set co-sign allow owner {WALLET}"));
        assert_eq!(
            unfreeze.line(),
            format!("remove co-sign freeze destination {WALLET}")
        );

        let subject = Hash([9; 32]);
        let after = ReadCoSignLists {
            cursor: Some(CoSignListEntry {
                list: CoSignList::Block,
                kind: CoSignSubjectKind::Asset,
                subject: subject.clone(),
            }),
        };
        assert_eq!(ReadCoSignLists::default().line(), "read co-sign lists");
        assert_eq!(
            after.line(),
            format!(
                "read co-sign lists after block asset {}",
                subject.to_base58()
            )
        );
    }
}
