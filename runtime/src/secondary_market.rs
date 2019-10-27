use support::{decl_module, decl_storage, decl_event, StorageValue, dispatch::Result,
	ensure, StorageMap};
use system::ensure_signed;
use parity_codec::{Encode, Decode};
use runtime_primitives::traits::{As, Hash, Zero};
use rstd::prelude::*;

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct BuyOrder<AccountId, Balance, Hash, BlockNumber>{
	issuer: AccountId,
	owner: AccountId,
	price: Balance,
	amount: u64,
	order_id: Hash,
	expiration: BlockNumber,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct SellOrder<AccountId, Balance, Hash, BlockNumber>{
	issuer: AccountId,
	owner: AccountId,
	price: Balance,
	amount: u64,
	order_id: Hash,
	expiration: BlockNumber,
}

pub trait Trait: balances::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
	trait Store for Module<T: Trait> as TemplateModule {

		/// A list of Accounts that have issued a share.
		/// This will only be populated when the Account starts to issue shares
		IssuerList get(issuer_list): Vec<T::AccountId>;

		/// The number of share that a given Account (company) has issued, and can be traded
		FloatingShares get(floating_shares): map T::AccountId => u64;

		/// The state in which the Account (company) is allowed to issue shares or not
		IsAllowedIssue get(is_allowed_issue): map T::AccountId => bool = false;

		/// The number of shares of the company the given Account owns (parameters: Holder, Issuer)
		OwnedShares get(owned_shares): map (T::AccountId, T::AccountId) => u64;

		/// The last traded price of the given company's share
		LastBidPrice get(last_bid_price): map T::AccountId => T::Balance;

		/// The maximum shares the given company can issue
		AuthorizedShares get(authorized_shares): map T::AccountId => u64;

		/// The market freeze state. Making this true will stop all further exchange
		MarketFreeze get(market_freeze): bool = false;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		pub fn put_sell_order(origin, issuer: T::AccountId, amount: u64, min_price: T::Balance, expiration: T::BlockNumber) -> Result {
			let sender = ensure_signed(origin)?;

			ensure!(Self::owned_shares((issuer, sender)) >= amount, "you do not own enough shares of this company");


			Ok(())
		}

		/// Give a given company the right to issue shares with the given authorized shares.
		/// You can only change the number of authorized shares through the change_authorized_shares function.
		pub fn give_issue_rights(origin, firm: T::AccountId, share_limit: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			// ensure that the firm is not giving themselves issue rights
			ensure!(sender != firm, "you cannot give rights to yourself");
			ensure!(Self::is_allowed_issue(&firm) == false, "the firm is already allowed to issue shares");

			let current_share_lim = Self::authorized_shares(&firm);

			// only add the given share limit when the current limit is 0
			if current_share_lim == 0 {
				// make sure the new limit value is more than 0
				ensure!(share_limit > 0, "the value must be greater than 0");

				<AuthorizedShares<T>>::insert(firm.clone(), share_limit);
			}

			// update the firm's issue right status to the blockchain
			<IsAllowedIssue<T>>::insert(firm.clone(), true);

			Self::deposit_event(RawEvent::GaveIssueRight(firm, share_limit));

			Ok(())
		}

		/// Revoke the right to issue shares for the given company (AccountId).
		/// This will not change the number of authorized shares or any floating shares.
		pub fn revoke_issue_rights(origin, firm: T::AccountId) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(sender != firm, "you cannot take rights to yourself");
			ensure!(Self::is_allowed_issue(&firm) == true, "the firm is already not allowed to issue shares");

			// update the firm's share issue right state
			<IsAllowedIssue<T>>::insert(firm.clone(), false);

			Self::deposit_event(RawEvent::RevokedIssueRight(firm));

			Ok(())
		}

		/// Changes the number of authorized shares for the given company (AccountId).
		/// You cannot decrease the authorized shares below the floating shares.
		/// This will not change the number of floating shares.
		pub fn change_authorized_shares(origin, firm: T::AccountId, new_limit: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(<IsAllowedIssue<T>>::get(&firm), "the firm is not allowed to issue shares");
			ensure!(sender != firm.clone(), "you cannot change your own issue limit");
			ensure!(new_limit > Self::floating_shares(&firm), "the firm cannot limit shares \
				less than the already issued amount");

			<AuthorizedShares<T>>::insert(firm.clone(), new_limit);

			Self::deposit_event(RawEvent::AdjustedAuthorizedStock(firm, new_limit));

			Ok(())
		}

		/// Issues the given amount of number of shares for the calling firm.
		/// You cannot issue more than the authorized amount. The newly issued shares
		/// will be directly sent to the calling firm
		pub fn issue_shares(origin, amount: u64) -> Result {
			let sender = ensure_signed(origin)?;

			ensure!(Self::is_allowed_issue(&sender), "this firm is not allowed to issue additional shares");

			let old_shares_outstanding = Self::floating_shares(&sender);

			//let new_shares_outstanding = Self::floating_shares(&sender) + amount.clone();
			let new_shares_outstanding = old_shares_outstanding.checked_add(amount.clone())
				.ok_or("overflowing when issuing new shares")?;

			ensure!(new_shares_outstanding < Self::authorized_shares(&sender), "already issued the maximum amount of shares");

			// add the issuer to the list if it is not in there
			if !Self::is_issuer(&sender) {
				<IssuerList<T>>::mutate(|account| account.push(sender.clone()));
			}

			<FloatingShares<T>>::insert(&sender, new_shares_outstanding);
			<OwnedShares<T>>::insert((sender.clone(), sender.clone()), amount.clone());

			Self::deposit_event(RawEvent::IssuedShares(sender, amount));

			Ok(())
		}

		/// Retires (burns) the given amount of shares that the calling firm owns
		/// when the floating shares becomes 0, the firm will be removed from the issuing list
		pub fn retire_shares(origin, amount: u64) -> Result {
			let sender = ensure_signed(origin)?;

			// get the caller's own shares
			let held_shares = Self::owned_shares((sender.clone(), sender.clone()));

			// prevent underflow by making sure that the account has more shares than the amount to decrease
			ensure!(held_shares >= amount, "you do not have enough shares to retire");

			// the new number of shares the caller will have
			//let new_amount = held_shares - amount;
			let new_amount = held_shares.checked_sub(amount).ok_or("underflow while subtracting held shares")?;

			// the new number of floating shares that is issued by the caller
			//let new_float = Self::floating_shares(&sender) - amount;
			let new_float = Self::floating_shares(&sender).checked_sub(amount).ok_or("underflow while subtracting floating shares")?;

			//todo: check and remove the firm's name from the IssuerList if the total floating share becomes 0
			if Self::is_issuer(&sender) && new_float == 0{
				let mut current_issuers = Self::issuer_list();
				current_issuers.retain(|x| x != &sender);

				<IssuerList<T>>::put(current_issuers);

			}

			<OwnedShares<T>>::insert((sender.clone(), sender.clone()), new_amount);
			<FloatingShares<T>>::insert(sender.clone(), new_float);

			Self::deposit_event(RawEvent::RetiredShares(sender, amount));

			Ok(())
		}

		pub fn freeze_market(origin) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			ensure!(!Self::market_freeze(), "the market is already frozen");

			<MarketFreeze<T>>::put(true);
			Self::deposit_event(RawEvent::MarketFrozen(sender, true));

			Ok(())
		}

		pub fn unfreeze_market(origin) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			ensure!(Self::market_freeze(), "the market is already not frozen");

			<MarketFreeze<T>>::put(false);
			Self::deposit_event(RawEvent::MarketFrozen(sender, false));

			Ok(())
		}
	}
}

// private functions for the runtime module
impl <T:Trait> Module<T> {
	fn is_issuer(firm: &T::AccountId) -> bool {
		<IssuerList<T>>::get().contains(firm)
	}

}

decl_event!(
	pub enum Event<T> where 
	AccountId = <T as system::Trait>::AccountId {
		GaveIssueRight(AccountId, u64),
		RevokedIssueRight(AccountId),
		MarketFrozen(AccountId, bool),
		AdjustedAuthorizedStock(AccountId, u64),
		IssuedShares(AccountId, u64),
		RetiredShares(AccountId, u64),


	}
);

