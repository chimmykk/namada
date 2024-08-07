use borsh::{BorshDeserialize, BorshSerialize};
use data_encoding::HEXUPPER;
use ibc::apps::nft_transfer::types::msgs::transfer::MsgTransfer as IbcMsgNftTransfer;
use ibc::apps::nft_transfer::types::packet::PacketData as NftPacketData;
use ibc::apps::nft_transfer::types::PORT_ID_STR as NFT_PORT_ID_STR;
use ibc::apps::transfer::types::msgs::transfer::MsgTransfer as IbcMsgTransfer;
use ibc::apps::transfer::types::packet::PacketData;
use ibc::apps::transfer::types::PORT_ID_STR as FT_PORT_ID_STR;
use ibc::core::channel::types::msgs::PacketMsg;
use ibc::core::channel::types::packet::Packet;
use ibc::core::handler::types::msgs::MsgEnvelope;
use ibc::core::host::types::identifiers::PortId;
use ibc::primitives::proto::Protobuf;
use masp_primitives::transaction::Transaction as MaspTransaction;
use namada_core::borsh::BorshSerializeExt;
use namada_token::Transfer;

/// The different variants of an Ibc message
#[derive(Debug, Clone)]
pub enum IbcMessage {
    /// Ibc Envelop
    Envelope(Box<MsgEnvelope>),
    /// Ibc transaprent transfer
    Transfer(MsgTransfer),
    /// NFT transfer
    NftTransfer(MsgNftTransfer),
}

/// IBC transfer message with `Transfer`
#[derive(Debug, Clone)]
pub struct MsgTransfer {
    /// IBC transfer message
    pub message: IbcMsgTransfer,
    /// Shieleded transfer for MASP transaction
    pub transfer: Option<Transfer>,
}

impl BorshSerialize for MsgTransfer {
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let encoded_msg = self.message.clone().encode_vec();
        let members = (encoded_msg, self.transfer.clone());
        BorshSerialize::serialize(&members, writer)
    }
}

impl BorshDeserialize for MsgTransfer {
    fn deserialize_reader<R: std::io::Read>(
        reader: &mut R,
    ) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};
        let (msg, transfer): (Vec<u8>, Option<Transfer>) =
            BorshDeserialize::deserialize_reader(reader)?;
        let message = IbcMsgTransfer::decode_vec(&msg)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        Ok(Self { message, transfer })
    }
}

/// IBC NFT transfer message with `Transfer`
#[derive(Debug, Clone)]
pub struct MsgNftTransfer {
    /// IBC NFT transfer message
    pub message: IbcMsgNftTransfer,
    /// Shieleded transfer for MASP transaction
    pub transfer: Option<Transfer>,
}

impl BorshSerialize for MsgNftTransfer {
    fn serialize<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let encoded_msg = self.message.clone().encode_vec();
        let members = (encoded_msg, self.transfer.clone());
        BorshSerialize::serialize(&members, writer)
    }
}

impl BorshDeserialize for MsgNftTransfer {
    fn deserialize_reader<R: std::io::Read>(
        reader: &mut R,
    ) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};
        let (msg, transfer): (Vec<u8>, Option<Transfer>) =
            BorshDeserialize::deserialize_reader(reader)?;
        let message = IbcMsgNftTransfer::decode_vec(&msg)
            .map_err(|err| Error::new(ErrorKind::InvalidData, err))?;
        Ok(Self { message, transfer })
    }
}

/// Shielding data in IBC packet memo
#[derive(Debug, Clone, BorshDeserialize, BorshSerialize)]
pub struct IbcShieldingData {
    /// MASP transaction for receiving the token
    pub shielding: Option<MaspTransaction>,
    /// MASP transaction for refunding the token
    pub refund: Option<MaspTransaction>,
}

impl From<IbcShieldingData> for String {
    fn from(data: IbcShieldingData) -> Self {
        HEXUPPER.encode(&data.serialize_to_vec())
    }
}

/// Extract MASP transaction from IBC envelope
pub fn extract_masp_tx_from_envelope(
    envelope: &MsgEnvelope,
) -> Option<MaspTransaction> {
    match envelope {
        MsgEnvelope::Packet(packet_msg) => match packet_msg {
            PacketMsg::Recv(msg) => {
                extract_masp_tx_from_packet(&msg.packet, false)
            }
            PacketMsg::Ack(msg) => {
                extract_masp_tx_from_packet(&msg.packet, true)
            }
            PacketMsg::Timeout(msg) => {
                extract_masp_tx_from_packet(&msg.packet, true)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Decode IBC shielding data from the memo string
pub fn decode_masp_tx_from_memo(
    memo: impl AsRef<str>,
) -> Option<IbcShieldingData> {
    let bytes = HEXUPPER.decode(memo.as_ref().as_bytes()).ok()?;
    IbcShieldingData::try_from_slice(&bytes).ok()
}

/// Extract MASP transaction from IBC packet memo
pub fn extract_masp_tx_from_packet(
    packet: &Packet,
    is_sender: bool,
) -> Option<MaspTransaction> {
    let port_id = if is_sender {
        &packet.port_id_on_a
    } else {
        &packet.port_id_on_b
    };
    let memo = extract_memo_from_packet(packet, port_id)?;
    let shielding_data = decode_masp_tx_from_memo(memo)?;
    if is_sender {
        shielding_data.refund
    } else {
        shielding_data.shielding
    }
}

fn extract_memo_from_packet(
    packet: &Packet,
    port_id: &PortId,
) -> Option<String> {
    match port_id.as_str() {
        FT_PORT_ID_STR => {
            let packet_data =
                serde_json::from_slice::<PacketData>(&packet.data).ok()?;
            if packet_data.memo.as_ref().is_empty() {
                None
            } else {
                Some(packet_data.memo.as_ref().to_string())
            }
        }
        NFT_PORT_ID_STR => {
            let packet_data =
                serde_json::from_slice::<NftPacketData>(&packet.data).ok()?;
            Some(packet_data.memo?.as_ref().to_string())
        }
        _ => {
            tracing::warn!(
                "Memo couldn't be extracted from the unsupported IBC packet \
                 data for Port ID {port_id}"
            );
            None
        }
    }
}

/// Get IBC memo string from MASP transaction for receiving
pub fn convert_masp_tx_to_ibc_memo(transaction: &MaspTransaction) -> String {
    let shielding_data = IbcShieldingData {
        shielding: Some(transaction.clone()),
        refund: None,
    };
    shielding_data.into()
}
