#![cfg_attr(not(feature = "std"), no_std)]

use codec::FullCodec;
use frame_support::{
	decl_error, decl_event, decl_module, decl_storage,
	traits::{Get, Currency, ExistenceRequirement, WithdrawReason},
	Parameter,
	debug
};

use sp_runtime::{
	traits::{CheckedConversion, Convert, SaturatedConversion, Member, AtLeast32Bit, MaybeSerializeDeserialize},
	DispatchResult, RuntimeDebug,
};
use sp_std::{
	collections::btree_map::BTreeMap,
	convert::{TryFrom, TryInto},
	marker::PhantomData,
	prelude::*,
	result,
	fmt::Debug,
};

use codec::{Decode, Encode};

use xcm::v0::{Error, Junction, MultiAsset, MultiLocation, Result};
use xcm_executor::traits::{FilterAssetLocation, LocationConversion, MatchesFungible, NativeAsset, TransactAsset};
use cumulus_primitives::ParaId;

#[derive(Encode, Decode, Eq, PartialEq, Clone, Copy, RuntimeDebug)]
/// Identity of chain.
pub enum ChainId {
	/// The relay chain.
	RelayChain,
	/// A parachain.
	ParaChain(ParaId),
}

#[derive(Encode, Decode, Eq, PartialEq, Clone, RuntimeDebug)]
/// Identity of cross chain currency.
pub struct PHAXCurrencyId {
	/// The reserve chain of the currency. For instance, the reserve chain of
	/// DOT is Polkadot.
	pub chain_id: ChainId,
	/// The identity of the currency.
	pub currency_id: Vec<u8>,
}

impl PHAXCurrencyId {
	pub fn new(chain_id: ChainId, currency_id: Vec<u8>) -> Self {
		PHAXCurrencyId { chain_id, currency_id }
	}
}

impl Into<MultiLocation> for PHAXCurrencyId {
	fn into(self) -> MultiLocation {
		MultiLocation::X1(Junction::GeneralKey(self.currency_id))
	}
}

impl Into<Vec<u8>> for PHAXCurrencyId {
	fn into(self) -> Vec<u8> {
		[ChainId::encode(&self.chain_id), self.currency_id].concat()
	}
}

/// Configuration trait of this pallet.
pub trait Trait: frame_system::Trait {
	/// Event type used by the runtime.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	type Balance: Parameter + Member + AtLeast32Bit + Default + Copy + MaybeSerializeDeserialize + Into<u128>;
	type Matcher: MatchesFungible<Self::Balance>;
	type AccountIdConverter: LocationConversion<Self::AccountId>;
	type XCurrencyIdConverter: XCurrencyIdConversion;
}

decl_storage! {
	trait Store for Module<T: Trait> as PhalaXCMAdapter {}
}

decl_event! (
	pub enum Event<T> where
		<T as frame_system::Trait>::AccountId,
		<T as Trait>::Balance,
	{
		/// Deposit asset into current chain. [currency_id, account_id, amount, to_tee]
		DepositAsset(Vec<u8>, AccountId, Balance, bool),

		/// Withdraw asset from current chain. [currency_id, account_id, amount, to_tee]
		WithdrawAsset(Vec<u8>, AccountId, Balance, bool),
	}
);

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        fn deposit_event() = default;

    }
}

impl<T> TransactAsset for Module<T> where 
    T: Trait,
{
    fn deposit_asset(asset: &MultiAsset, location: &MultiLocation) -> Result {
		debug::info!("------------------------------------------------");
		debug::info!(">>> trying deposit. asset: {:?}, location: {:?}", asset, location);

		let who = T::AccountIdConverter::from_location(location).ok_or(())?;
		debug::info!("who: {:?}", who);
		let currency_id = T::XCurrencyIdConverter::from_asset_and_location(asset, location).ok_or(())?;
		debug::info!("currency_id: {:?}", currency_id);
		let amount = T::Matcher::matches_fungible(&asset).ok_or(())?.saturated_into();
		debug::info!("amount: {:?}", amount);
		let balance_amount = amount.try_into().map_err(|_| ())?;
		debug::info!("balance amount: {:?}", balance_amount);
        
        Self::deposit_event(
            Event::<T>::DepositAsset(currency_id.clone().into(), who, balance_amount, true),
        );

		debug::info!(">>> success deposit.");
		debug::info!("------------------------------------------------");
		Ok(())
    }
    
    fn withdraw_asset(asset: &MultiAsset, location: &MultiLocation) -> result::Result<MultiAsset, Error> {
		debug::info!("------------------------------------------------");
		debug::info!(">>> trying withdraw. asset: {:?}, location: {:?}", asset, location);
		
		let who = T::AccountIdConverter::from_location(location).ok_or(())?;
		debug::info!("who: {:?}", who);
		let currency_id = T::XCurrencyIdConverter::from_asset_and_location(asset, location).ok_or(())?;
		debug::info!("currency_id: {:?}", currency_id);
		let amount = T::Matcher::matches_fungible(&asset).ok_or(())?.saturated_into();
		debug::info!("amount: {:?}", amount);
		let balance_amount = amount.try_into().map_err(|_| ())?;
		debug::info!("balance amount: {:?}", balance_amount);

        Self::deposit_event(
            Event::<T>::WithdrawAsset(currency_id.clone().into(), who, balance_amount, true),
        );

		debug::info!(">>> success withdraw.");
		debug::info!("------------------------------------------------");
		Ok(asset.clone())	
	}
}

pub struct IsConcreteWithGeneralKey<CurrencyId, FromRelayChainBalance>(
	PhantomData<(CurrencyId, FromRelayChainBalance)>,
);
impl<CurrencyId, B, FromRelayChainBalance> MatchesFungible<B>
	for IsConcreteWithGeneralKey<CurrencyId, FromRelayChainBalance>
where
	CurrencyId: TryFrom<Vec<u8>>,
	B: TryFrom<u128>,
	FromRelayChainBalance: Convert<u128, u128>,
{
	fn matches_fungible(a: &MultiAsset) -> Option<B> {
		if let MultiAsset::ConcreteFungible { id, amount } = a {
			if id == &MultiLocation::X1(Junction::Parent) {
				// Convert relay chain decimals to local chain
				let local_amount = FromRelayChainBalance::convert(*amount);
				return CheckedConversion::checked_from(local_amount);
			}
			if let Some(Junction::GeneralKey(key)) = id.last() {
				if TryInto::<CurrencyId>::try_into(key.clone()).is_ok() {
					return CheckedConversion::checked_from(*amount);
				}
			}
		}
		None
	}
}

pub trait XCurrencyIdConversion {
	fn from_asset_and_location(asset: &MultiAsset, location: &MultiLocation) -> Option<PHAXCurrencyId>;
}

pub struct XCurrencyIdConverter<NativeTokens>(
	PhantomData<NativeTokens>,
);
impl <NativeTokens: Get<BTreeMap<Vec<u8>, MultiLocation>>>  XCurrencyIdConversion for XCurrencyIdConverter<NativeTokens>
{
	fn from_asset_and_location(multi_asset: &MultiAsset, multi_location: &MultiLocation) -> Option<PHAXCurrencyId> {
		if let MultiAsset::ConcreteFungible { ref id, .. } = multi_asset {
			if id == &MultiLocation::X1(Junction::Parent) {
				let relaychain_currency : PHAXCurrencyId = PHAXCurrencyId {
					chain_id: ChainId::RelayChain,
					currency_id: b"DOT".to_vec(),
				};
				return Some(relaychain_currency);
			}

			if let Some(Junction::GeneralKey(key)) = id.last() {
				if NativeTokens::get().contains_key(&key.clone()) {
					// here we can trust the currency matchs the parachain, case NativePalletAssetOr already check this
					if let MultiLocation::X2(Junction::Parent, Junction::Parachain {id: paraid}) = NativeTokens::get().get(&key.clone()).unwrap() {
						let parachain_currency: PHAXCurrencyId = PHAXCurrencyId {
							chain_id: ChainId::ParaChain((*paraid).into()),
							currency_id: key.clone(),
						};
						return Some(parachain_currency);
					}
				}
			}
		}
		None
	}
}

pub struct NativePalletAssetOr<NativeTokens>(PhantomData<NativeTokens>);
impl<NativeTokens: Get<BTreeMap<Vec<u8>, MultiLocation>>> FilterAssetLocation for NativePalletAssetOr<NativeTokens> {
	fn filter_asset_location(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		if NativeAsset::filter_asset_location(asset, origin) {
			return true;
		}

		// native asset identified by a general key
		if let MultiAsset::ConcreteFungible { ref id, .. } = asset {
			if let Some(Junction::GeneralKey(key)) = id.last() {
				if NativeTokens::get().contains_key(&key.clone()) {
					return (*origin) == *(NativeTokens::get().get(&key.clone()).unwrap());
				}
			}
		}

		false
	}
}

pub trait XcmHandler {
	type Origin;
	type Xcm;
	fn execute(origin: Self::Origin, xcm: Self::Xcm) -> DispatchResult;
}