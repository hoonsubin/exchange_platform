use support::{decl_module, decl_storage, decl_event, StorageValue, dispatch::Result,
	ensure, StorageMap};
use system::ensure_signed;

pub trait Trait: balances::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
	trait Store for Module<T: Trait> as TemplateModule {

		/// The array of Accounts that have issued a share.
		/// This will only be populated when the Account starts to issue shares
		IssuerArray get(issuer_array): map u64 => T::AccountId;

		/// The number of share that a given Account (company) has issued
		TotalIssuedShares get(total_issued_shares): map T::AccountId => u64;

		/// The state in which the Account (company) is allowed to issue shares or not
		IsAllowedIssue get(is_allowed_issue): map T::AccountId => bool = false;

		/// The number of shares of the company the given Account owns (parameters: Issuer, Holder)
		OwnedShares get(owned_shares): map (T::AccountId, T::AccountId) => u64;

		/// The last traded price of the given company's share
		LastBidPrice get(last_bid_price): map T::AccountId => T::Balance;

		/// The maximum shares the given company can issue
		MaxShare get(max_share): map T::AccountId => u64;

		/// The market freeze state. Making this true will stop all further exchange
		MarketFreeze get(market_freeze): bool = false;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		pub fn give_issue_rights(origin, firm: T::AccountId, share_limit: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			ensure!(sender != firm, "you cannot give rights to yourself");
			ensure!(Self::is_allowed_issue(&firm) == false, "the firm is already allowed to issue shares");

			let current_share_lim = Self::max_share(&firm);

			// only add the given share limit when the current limit is 0
			if current_share_lim == 0 {
				// make sure the new limit value is more than 0
				ensure!(share_limit > 0, "the value must be greater than 0");

				<MaxShare<T>>::insert(firm.clone(), share_limit);
			}

			<IsAllowedIssue<T>>::insert(firm.clone(), true);

			Self::deposit_event(RawEvent::GaveIssueRight(firm, share_limit));

			Ok(())
		}

		pub fn revoke_issue_rights(origin, firm: T::AccountId) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(sender != firm, "you cannot take rights to yourself");
			ensure!(Self::is_allowed_issue(&firm) == true, "the firm is already not allowed to issue shares");

			<IsAllowedIssue<T>>::insert(firm.clone(), false);

			Self::deposit_event(RawEvent::RevokedIssueRight(firm));

			Ok(())
		}

		pub fn change_share_limit(origin, firm: T::AccountId, new_limit: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(<IsAllowedIssue<T>>::get(&firm), "the firm is not allowed to issue shares");
			ensure!(sender != firm.clone(), "you cannot change your own issue limit");
			ensure!(new_limit > Self::total_issued_shares(&firm), "the firm cannot limit shares \
				less than the already issued amount");

			<MaxShare<T>>::insert(firm.clone(), new_limit);

			Self::deposit_event(RawEvent::ChangedIssueLimit(firm, new_limit));

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

decl_event!(
	pub enum Event<T> where 
	AccountId = <T as system::Trait>::AccountId {
		GaveIssueRight(AccountId, u64),
		RevokedIssueRight(AccountId),
		MarketFrozen(AccountId, bool),
		ChangedIssueLimit(AccountId, u64),
	}
);

