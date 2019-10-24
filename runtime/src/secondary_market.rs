use support::{decl_module, decl_storage, decl_event, dispatch::Result};
use system::ensure_signed;

/// The module's configuration trait.
pub trait Trait: system::Trait {
    /// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
	trait Store for Module<T: Trait> as SecondaryMarket {
		Something get(something): Option<u32>;

		/// The array of Accounts that have issued a share
		IssuerArray get(issuer_array): map u64 => T::AccountId;

		/// The number of share that a given Account (company) has issued
		IssuedShares get(issued_shares): map T::AccountId => u64;

		/// The state in which the Account (company) is allowed to issue shares or not
		IsAllowedIssue get(is_allowed_issue): map T::AccountId => bool;

		/// The number of shares of the company the given Account owns (parameters are Issuer, Holder)
		OwnedShares get(owned_shares): map (T::AccountId, T::AccountId) => u64;

		/// The last traded price of the given company's share
		LastBidPrice get(last_bid_price): map T::AccountId => T::Balance;
	}
}

decl_module! {
    /// The module declaration.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        /// Initializing events
		/// this is needed only if you are using events in your module
		fn deposit_event() = default;

		pub fn do_something(origin, something: u32) -> Result {
			let who = ensure_signed(origin)?;

			Something::put(something);
			Self::deposit_event(RawEvent::SomethingStored(something, who));
			Ok(())
		}

		pub fn allow_issue(origin, firm: T::AccountId) -> Result {
			let sender = ensure_signed(origin)?;
			

			Ok(())
		}


	}
}

decl_event!(
	pub enum Event<T> where 
		AccountId = <T as system::Trait>::AccountId {

			SomethingStored(u32, AccountId),
			
	}
);