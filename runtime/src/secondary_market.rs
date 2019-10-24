use support::{decl_module, decl_storage, decl_event, StorageValue, dispatch::Result,
	ensure, StorageMap};
use system::ensure_signed;

pub trait Trait: balances::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
	trait Store for Module<T: Trait> as TemplateModule {
		Something get(something): Option<u32>;

		/// The array of Accounts that have issued a share. This will only be populated when the Account starts to issue shares
		IssuerArray get(issuer_array): map u64 => T::AccountId;

		/// The number of share that a given Account (company) has issued
		IssuedShares get(issued_shares): map T::AccountId => u64;

		/// The state in which the Account (company) is allowed to issue shares or not
		IsAllowedIssue get(is_allowed_issue): map T::AccountId => bool = false;

		/// The number of shares of the company the given Account owns (parameters: Issuer, Holder)
		OwnedShares get(owned_shares): map (T::AccountId, T::AccountId) => u64;

		/// The last traded price of the given company's share
		LastBidPrice get(last_bid_price): map T::AccountId => T::Balance;

		/// The maximum shares the given company can issue
		MaxShare get(max_share): map T::AccountId => u64;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		pub fn do_something(origin, something: u32) -> Result {

			let who = ensure_signed(origin)?;

			<Something<T>>::put(something);

			Self::deposit_event(RawEvent::SomethingStored(something, who));
			Ok(())
		}

		pub fn give_issue_rights(origin, firm: T::AccountId, share_limit: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			ensure!(sender != firm, "you cannot give rights to yourself");

			let firm_state = Self::is_allowed_issue(firm.clone());
			ensure!(firm_state == false, "the firm is already allowed to issue shares");
			ensure!(share_limit > 0, "the value must be greater than 0");

			<IsAllowedIssue<T>>::insert(firm.clone(), true);
			<MaxShare<T>>::insert(firm, share_limit);

			Ok(())
		}

		pub fn revoke_issue_rights(origin, firm: T::AccountId) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(sender != firm, "you cannot take rights to yourself");

			let firm_state = Self::is_allowed_issue(firm.clone());
			ensure!(firm_state == true, "the firm is already not allowed to issue shares");

			<IsAllowedIssue<T>>::insert(firm.clone(), false);

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

/// tests for this module
#[cfg(test)]
mod tests {
	use super::*;

	use runtime_io::with_externalities;
	use primitives::{H256, Blake2Hasher};
	use support::{impl_outer_origin, assert_ok};
	use runtime_primitives::{
		BuildStorage,
		traits::{BlakeTwo256, IdentityLookup},
		testing::{Digest, DigestItem, Header}
	};

	impl_outer_origin! {
		pub enum Origin for Test {}
	}

	// For testing the module, we construct most of a mock runtime. This means
	// first constructing a configuration type (`Test`) which `impl`s each of the
	// configuration traits of modules we want to use.
	#[derive(Clone, Eq, PartialEq)]
	pub struct Test;
	impl system::Trait for Test {
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type Digest = Digest;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
		type Log = DigestItem;
	}
	impl Trait for Test {
		type Event = ();
	}
	type SecondaryMarket = Module<Test>;

	// This function basically just builds a genesis storage key/value store according to
	// our desired mockup.
	fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
		system::GenesisConfig::<Test>::default().build_storage().unwrap().0.into()
	}

	#[test]
	fn it_works_for_default_value() {
		with_externalities(&mut new_test_ext(), || {
			// Just a dummy test for the dummy funtion `do_something`
			// calling the `do_something` function with a value 42
			assert_ok!(SecondaryMarket::do_something(Origin::signed(1), 42));
			// asserting that the stored value is equal to what we stored
			assert_eq!(SecondaryMarket::something(), Some(42));
		});
	}
}
