// The goal of staking precompile is to allow interaction between EVM users and smart contracts and
// subtensor staking functionality, namely add_stake, and remove_stake extrinsicsk, as well as the
// staking state.
//
// Additional requirement is to preserve compatibility with Ethereum indexers, which requires
// no balance transfers from EVM accounts without a corresponding transaction that can be
// parsed by an indexer.
//
// Implementation of add_stake:
//   - User transfers balance that will be staked to the precompile address with a payable
//     method addStake. This method also takes hotkey public key (bytes32) of the hotkey
//     that the stake should be assigned to.
//   - Precompile transfers the balance back to the signing address, and then invokes
//     do_add_stake from subtensor pallet with signing origin that mmatches to HashedAddressMapping
//     of the message sender, which will effectively withdraw and stake balance from the message
//     sender.
//   - Precompile checks the result of do_add_stake and, in case of a failure, reverts the transaction,
//     and leaves the balance on the message sender account.
//
// Implementation of remove_stake:
//   - User involkes removeStake method and specifies hotkey public key (bytes32) of the hotkey
//     to remove stake from, and the amount to unstake.
//   - Precompile calls do_remove_stake method of the subtensor pallet with the signing origin of message
//     sender, which effectively unstakes the specified amount and credits it to the message sender
//   - Precompile checks the result of do_remove_stake and, in case of a failure, reverts the transaction.
//

use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
use frame_system::RawOrigin;
use pallet_evm::{
    AddressMapping, BalanceConverter, ExitError, PrecompileFailure, PrecompileHandle,
};
use precompile_utils::EvmResult;
use sp_core::{H256, U256};
use sp_runtime::traits::{Dispatchable, StaticLookup, UniqueSaturatedInto};
use sp_std::vec;
use subtensor_runtime_common::ProxyType;

use crate::{PrecompileExt, PrecompileHandleExt};

// Old StakingPrecompile had ETH-precision in values, which was not alligned with Substrate API. So
// it's kinda deprecated, but exists for backward compatibility. Eventually, we should remove it
// to stop supporting both precompiles.
//
// All the future extensions should happen in StakingPrecompileV2.
pub(crate) struct StakingPrecompileV2<R>(PhantomData<R>);

impl<R> PrecompileExt<R::AccountId> for StakingPrecompileV2<R>
where
    R: frame_system::Config
        + pallet_evm::Config
        + pallet_network::Config
        + pallet_proxy::Config<ProxyType = ProxyType>,
    R::AccountId: From<[u8; 32]> + Into<[u8; 32]>,
    <R as frame_system::Config>::RuntimeCall: From<pallet_network::Call<R>>
        + From<pallet_proxy::Call<R>>
        + GetDispatchInfo
        + Dispatchable<PostInfo = PostDispatchInfo>,
    <R as pallet_evm::Config>::AddressMapping: AddressMapping<R::AccountId>,
    <<R as frame_system::Config>::Lookup as StaticLookup>::Source: From<R::AccountId>,
{
    const INDEX: u64 = 2053;
}

#[precompile_utils::precompile]
impl<R> StakingPrecompileV2<R>
where
  R: frame_system::Config
    + pallet_evm::Config
    + pallet_network::Config
    + pallet_proxy::Config<ProxyType = ProxyType>,
  R::AccountId: From<[u8; 32]> + Into<[u8; 32]>,
  <R as frame_system::Config>::RuntimeCall: From<pallet_network::Call<R>>
    + From<pallet_proxy::Call<R>>
    + GetDispatchInfo
    + Dispatchable<PostInfo = PostDispatchInfo>,
  <R as pallet_evm::Config>::AddressMapping: AddressMapping<R::AccountId>,
  <<R as frame_system::Config>::Lookup as StaticLookup>::Source: From<R::AccountId>,
{
  #[precompile::public("addToStake(bytes32,uint256,uint256,uint256,uint256,uint256)")]
  #[precompile::payable]
  fn add_to_stake(
    handle: &mut impl PrecompileHandle,
    address: H256,
    subnet_id: U256,
    subnet_node_id: U256,
    hotkey: U256,
    stake_to_be_added: U256,
  ) -> EvmResult<()> {
    let account_id = handle.caller_account_id::<R>();
    let stake_to_be_added = stake_to_be_added.unique_saturated_into();
    let hotkey = R::AccountId::from(address.0);
    let subnet_id = try_u256_to_u32(subnet_id)?;
    let call = pallet_network::Call::<R>::add_to_stake {
      subnet_id
      subnet_node_id
      hotkey
      stake_to_be_added
    };

    handle.try_dispatch_runtime_call::<R, _>(call, RawOrigin::Signed(account_id))
  }
}

fn try_u256_to_u32(value: U256) -> Result<u32, PrecompileFailure> {
  value.try_into().map_err(|_| PrecompileFailure::Error {
    exit_status: ExitError::Other("u32 out of bounds".into()),
  })
}