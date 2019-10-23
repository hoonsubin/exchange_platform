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
        Theum get(tetsting): Hello;
	}
}

decl_module! {
    /// The module declaration.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Initializing events
		// this is needed only if you are using events in your module
		fn deposit_event() = default;

		pub fn do_something(origin, something: u32) -> Result {
			let who = ensure_signed(origin)?;

			Something::put(something);
			Self::deposit_event(RawEvent::SomethingStored(something, who));
			Ok(())
		}
	}
}

decl_event!(
	pub enum Event<T> where AccountId = <T as system::Trait>::AccountId {
		SomethingStored(u32, AccountId),
	}
);