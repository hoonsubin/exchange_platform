use support::{decl_module, decl_storage, decl_event, StorageValue, dispatch::Result,
	ensure, StorageMap, traits::{Currency, ReservableCurrency}};
use system::ensure_signed;
use parity_codec::{Encode, Decode};
use runtime_primitives::traits::{As, Hash, CheckedMul};
use rstd::prelude::*;
use runtime_io::{self};

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct BuyOrder<AccountId, Balance, Hash>{
	firm: AccountId,
	owner: AccountId,
	max_price: Balance,
	amount: u64,
	order_id: Hash,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct SellOrder<AccountId, Balance, Hash>{
	firm: AccountId,
	owner: AccountId,
	min_price: Balance,
	amount: u64,
	order_id: Hash,
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

		/// The number of shares of the company the given Account owns (parameters: Holder, firm)
		OwnedShares get(owned_shares): map (T::AccountId, T::AccountId) => u64;

		/// The last traded price of the given company's share. This is used to track market price
		LastBidPrice get(last_bid_price): map T::AccountId => T::Balance;

		/// The maximum shares the given company can issue
		AuthorizedShares get(authorized_shares): map T::AccountId => u64;

		/// The market freeze state. Making this true will stop all further exchange
		MarketFreeze get(market_freeze): bool = false;

		/// The list of sell orders for a particular company's account
		SellOrdersList get(sell_order_list): map T::AccountId => Vec<SellOrder<T::AccountId, T::Balance, T::Hash>>;

		/// The list of buy orders for a particular company's account
		BuyOrdersList get(buy_orders_list): map T::AccountId => Vec<BuyOrder<T::AccountId, T::Balance, T::Hash>>;

		/// A nonce value used for generating random values
		Nonce get(nonce): u64 = 0;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		/// Searches the current buy orders and see if there is a price match for the transaction.
		/// If there are no buy orders, this will create a new sell order which will be checked by the
		/// other traders who call put_buy_order function.
		pub fn put_sell_order(origin, firm: T::AccountId, amount: u64, min_price: T::Balance) -> Result {
			//todo: the current implementation of this code is very inefficient, please refactor later
			let sender = ensure_signed(origin)?;

			ensure!(!Self::market_freeze(), "[Error]the market is frozen right now");
			ensure!(Self::balance_to_u64(min_price.clone()) > 0, "[Error]you cannot sell for 0");
			ensure!(Self::owned_shares((firm.clone(), sender.clone())) >= amount
			, "[Error]you do not own enough shares of this company");
			
			// get the entire buy orders from the blockchain
			// we only call this once to save memory
			let buy_orders_list = Self::buy_orders_list(&firm);

			// a mutable copy of the list that we will be making changes to
			let mut temp_buy_orders_list = buy_orders_list.clone();

			// this is also a clone of the master list, but we will not make changes to this one
			let mut new_order_list = buy_orders_list.clone();

			// make a new list of all the orders that are not going to be mutated
			new_order_list.retain(|x| x.max_price > min_price);

			// only get the orders where the max price is lower or equal to the min price, and is not expired
			temp_buy_orders_list.retain(|x| x.max_price <= min_price);

			// check if the number orders are greater than 0
			if temp_buy_orders_list.len() > 0 {
				let mut remaining_shares = amount;

				// sort the vector from lower max_price to high
				//todo: requires check if this actually works well
				temp_buy_orders_list.sort_by(|a, b| a.max_price.cmp(&b.max_price));
				
				runtime_io::print("[Debug]found a good list of buy orders");
				// we are cloning the master list because we will be making changes to it during the loop
				for order in temp_buy_orders_list.clone() {
					// check if the order of the share is enough
					if remaining_shares != 0 && order.amount >= remaining_shares {
						// if the buyer's amount is smaller than the seller's
						// buyer will first send the amount (order.max_price) * (order.amount) to the seller
						// note that we are selling the shares for the buyer's requested price, not the caller's
						let total_price = order.max_price.checked_mul(&Self::u64_to_balance(order.amount))
							.ok_or("[Error]overflow in calculating total price")?;

						// transfer the total price from the owner of the order, to the caller
						// note that we are sending the money first to prevent any errors before changing the share value
						<balances::Module<T> as Currency<_>>::transfer(&order.owner, &sender, total_price)?;

						// then send all the buyer's requested amount to the buyer
						Self::transfer_share(sender.clone(), order.owner.clone(), firm.clone(), order.amount)?;

						// update the last bid price for this share
						<LastBidPrice<T>>::insert(firm.clone(), order.max_price);
						
						// match the reaming shares, and return 0 when overflow
						remaining_shares = match remaining_shares.checked_sub(order.amount){
							Some(v) => v,
							None => 0,
						};

						// remove the current order from the master list once the transaction is done
						temp_buy_orders_list.retain(|x| x.order_id != order.order_id);

					}
					else { // if the amount of buy is greater than the amount to sell
						let shares_selling = order.amount.checked_sub(remaining_shares)
							.ok_or("[Error]underflow during calculation of total shares")?;

						// calculate the total price the owner of the order will have to send
						let total_price = order.max_price.checked_mul(&Self::u64_to_balance(shares_selling))
							.ok_or("[Error]overflow in calculating total price")?;
						
						<balances::Module<T> as Currency<_>>::transfer(&order.owner, &sender, total_price)?;

						if shares_selling > 0 {
							// if buy quantity > sell quantity, make a new order with adjusted amount
							// and replace it with the old one. This will also give a new Hash (order_id)
							let adjusted_buy_order = BuyOrder {
								firm: order.firm,
								owner: order.owner,
								max_price: order.max_price,
								amount: shares_selling,
								order_id: Self::generate_hash(sender.clone())

							};

							// push (add to the last index) the newly adjusted buy order to the master list
							temp_buy_orders_list.push(adjusted_buy_order);
						}
						
						// break out of the for loop once the caller sold all the shares
						break;
					}
					
				}
				// combine the order list that wasn't touched, and the adjusted ones
				new_order_list.append(&mut temp_buy_orders_list);

				// as a final check, only retain the items where the quantity is over 0
				new_order_list.retain(|x| x.amount > 0);

				// replace the entire list with the new one
				<BuyOrdersList<T>>::insert(&firm, new_order_list);

				// if the caller could not sell all the shares
				if remaining_shares > 0 {
					Self::add_sell_order_to_blockchain(sender.clone(), firm.clone(), sender.clone(), min_price, remaining_shares);
				}
			}
			else { // if there are no existing buy orders with the right price in the market

				runtime_io::print("[Debug]no buy orders in given price point, will make a new sell order");
				// create a new sell order so later buyers can check it
				Self::add_sell_order_to_blockchain(sender.clone(), firm.clone(), sender.clone(), min_price, amount);
			}
			Ok(())
		}

		/// Searches the current sell orders and see if there is a price match for the transaction.
		/// If there are no sell orders, this will create a new buy order which will be checked by the
		/// other traders who call put_sell_order function.
		pub fn put_buy_order(origin, firm: T::AccountId, amount: u64, max_price: T::Balance) -> Result {
			//todo: the current implementation of this code is very inefficient, please refactor later
			let sender = ensure_signed(origin)?;
			let total_price = max_price.checked_mul(&Self::u64_to_balance(amount.clone()))
				.ok_or("[Error]overflow in calculating total price")?;

			ensure!(!Self::market_freeze(), "[Error]the market is frozen right now");
			ensure!(<balances::Module<T>>::free_balance(sender.clone()) >= total_price
			, "[Error]you don't have enough free balance for this trade");

			let sell_order_list = Self::sell_order_list(&firm);

			let mut temp_sell_list = sell_order_list.clone();
			let mut new_sell_list = sell_order_list.clone();

			// make a new list of all the orders that are not going to be mutated
			new_sell_list.retain(|x| x.min_price > max_price);

			// only get the orders where the min price is lower or equal to the max price
			// we will be making adjustments to this list to update the global list
			temp_sell_list.retain(|x| x.min_price <= max_price);

			// check if the number of valid orders are greater than 0
			if temp_sell_list.len() > 0 {
				let mut remaining_shares_to_buy = amount;

				// sort the vector from highest min_price to low
				//todo: requires check if this actually works well
				temp_sell_list.sort_by(|a, b| b.min_price.cmp(&a.min_price));

				for order in temp_sell_list.clone(){

					if remaining_shares_to_buy != 0 && order.amount >= remaining_shares_to_buy {
						// get the total price the buyer will have to pay for the current order
						let total_price = order.min_price.checked_mul(&Self::u64_to_balance(order.amount))
						.ok_or("[Error]overflow in calculating total price")?;
						
						ensure!(Self::owned_shares((order.owner.clone(), order.firm.clone())) >= order.amount
						, "[Error]the seller does not have enough shares");

						// transfer the total price from the owner of the order, to the caller
						// note that we are sending the money first to prevent any errors before changing the share value
						<balances::Module<T> as Currency<_>>::transfer(&sender, &order.owner, total_price)?;

						// then send all the buyer's requested amount to the buyer
						Self::transfer_share(order.owner.clone(), sender.clone(), firm.clone(), order.amount)?;

						// update the last bid price for this share
						<LastBidPrice<T>>::insert(firm.clone(), order.min_price);

						// finally change the remaining share value
						// match the reaming shares, and return 0 when overflow
						remaining_shares_to_buy = match remaining_shares_to_buy.checked_sub(order.amount){
							Some(v) => v,
							None => 0,
						};
						// remove the current order from the master list once the transaction is done
						temp_sell_list.retain(|x| x.order_id != order.order_id);
					}
					else { // if the amount of buy is greater than the amount to sell
						let shares_buying = order.amount.checked_sub(remaining_shares_to_buy)
							.ok_or("[Error]underflow during calculation of total shares")?;

						// calculate the total price the caller will have to send
						let total_price = order.min_price.checked_mul(&Self::u64_to_balance(shares_buying))
							.ok_or("[Error]overflow in calculating total price")?;
						
						<balances::Module<T> as Currency<_>>::transfer(&sender, &order.owner, total_price)?;

						if shares_buying > 0 {
							// if sell quantity > your buy quantity, make a new order with adjusted amount
							// and replace it with the old one. This will also give a new Hash (order_id)
							let adjusted_sell_order = SellOrder {
								firm: order.firm,
								owner: order.owner,
								min_price: order.min_price,
								amount: shares_buying,
								order_id: Self::generate_hash(sender.clone())

							};

							// push (add to the last index) the newly adjusted sell order to the master list
							temp_sell_list.push(adjusted_sell_order);
						}
						
						// break out of the for loop to combine the adjusted list
						break;
					}
				}
				// combine the order list that wasn't touched, and the adjusted ones
				new_sell_list.append(&mut temp_sell_list);

				// as a final check, only retain the items where the quantity is over 0
				new_sell_list.retain(|x| x.amount > 0);

				// replace the entire list with the new one
				<SellOrdersList<T>>::insert(&firm, new_sell_list);

				// if the caller could not sell all the shares
				if remaining_shares_to_buy > 0 {
					Self::add_sell_order_to_blockchain(sender.clone(), firm.clone(), sender.clone(), max_price, remaining_shares_to_buy);
				}

			}
			else { // if there are no good orders in the market
				runtime_io::print("[Debug]no sell orders in given price point, will make a new buy order");
				// create a new sell order so later buyers can check it
				Self::add_buy_order_to_blockchain(sender.clone(), firm.clone(), sender.clone(), max_price, amount);
			}

			
			Ok(())
		}

		/// Give a given company the right to issue shares with the given authorized shares.
		/// You can only change the number of authorized shares through the change_authorized_shares function.
		pub fn give_issue_rights(origin, firm: T::AccountId, authorized_shares: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			// ensure that the firm is not giving themselves issue rights
			ensure!(sender != firm, "[Error]you cannot give rights to yourself");
			ensure!(Self::is_allowed_issue(&firm) == false, "[Error]the firm is already allowed to issue shares");

			let current_share_lim = Self::authorized_shares(&firm);

			// only add the given share limit when the current limit is 0
			if current_share_lim == 0 {
				// make sure the new limit value is more than 0
				ensure!(authorized_shares > 0, "[Error]the value must be greater than 0");

				<AuthorizedShares<T>>::insert(firm.clone(), authorized_shares);
			}

			// update the firm's issue right status to the blockchain
			<IsAllowedIssue<T>>::insert(firm.clone(), true);

			Self::deposit_event(RawEvent::GaveFirmIssueRight(firm, authorized_shares));

			Ok(())
		}

		/// Revoke the right to issue shares for the given company (AccountId).
		/// This will not change the number of authorized shares or any floating shares.
		pub fn revoke_issue_rights(origin, firm: T::AccountId) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(sender != firm, "[Error]you cannot take rights to yourself");
			ensure!(Self::is_allowed_issue(&firm) == true, "[Error]the firm is already not allowed to issue shares");

			// update the firm's share issue right state
			<IsAllowedIssue<T>>::insert(firm.clone(), false);

			Self::deposit_event(RawEvent::RevokedFirmIssueRight(firm));

			Ok(())
		}

		/// Changes the number of authorized shares for the given company (AccountId).
		/// You cannot decrease the authorized shares below the floating shares.
		/// This will not change the number of floating shares.
		pub fn change_authorized_shares(origin, firm: T::AccountId, new_limit: u64) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;
			ensure!(<IsAllowedIssue<T>>::get(&firm), "[Error]the firm is not allowed to issue shares");
			ensure!(sender != firm.clone(), "[Error]you cannot change your own issue limit");
			ensure!(new_limit > Self::floating_shares(&firm), "[Error]the firm cannot limit shares \
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

			ensure!(Self::is_allowed_issue(&sender), "[Error]this firm is not allowed to issue additional shares");

			let old_shares_outstanding = Self::floating_shares(&sender);

			//let new_shares_outstanding = Self::floating_shares(&sender) + amount.clone();
			let new_shares_outstanding = old_shares_outstanding.checked_add(amount.clone())
				.ok_or("[Error]overflowing when issuing new shares")?;

			ensure!(new_shares_outstanding < Self::authorized_shares(&sender)
			, "[Error]already issued the maximum amount of shares");

			// add the firm to the list if it is not in there
			if !Self::is_firm(&sender) {
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
			ensure!(held_shares >= amount, "[Error]you do not have enough shares to retire");

			// the new number of shares the caller will have
			//let new_amount = held_shares - amount;
			let new_amount = held_shares.checked_sub(amount).ok_or("[Error]underflow while subtracting held shares")?;

			// the new number of floating shares that is issued by the caller
			//let new_float = Self::floating_shares(&sender) - amount;
			let new_float = Self::floating_shares(&sender).checked_sub(amount)
				.ok_or("[Error]underflow while subtracting floating shares")?;

			//todo: check and remove the firm's name from the IssuerList if the total floating share becomes 0
			if Self::is_firm(&sender) && new_float == 0{
				let mut current_firms = Self::issuer_list();
				current_firms.retain(|x| x != &sender);

				<IssuerList<T>>::put(current_firms);

			}

			<OwnedShares<T>>::insert((sender.clone(), sender.clone()), new_amount);
			<FloatingShares<T>>::insert(sender.clone(), new_float);

			Self::deposit_event(RawEvent::RetiredShares(sender, amount));

			Ok(())
		}

		pub fn freeze_market(origin) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			ensure!(!Self::market_freeze(), "[Error]the market is already frozen");

			<MarketFreeze<T>>::put(true);
			Self::deposit_event(RawEvent::MarketFrozen(sender, true));

			Ok(())
		}

		pub fn unfreeze_market(origin) -> Result {
			// todo: make this ensure that origin is root
			let sender = ensure_signed(origin)?;

			ensure!(Self::market_freeze(), "[Error]the market is already not frozen");

			<MarketFreeze<T>>::put(false);
			Self::deposit_event(RawEvent::MarketFrozen(sender, false));

			Ok(())
		}
	}
}

// private functions for the runtime module
impl <T:Trait> Module<T> {

	/// checks if the given AccountId has share issue rights
	fn is_firm(firm: &T::AccountId) -> bool {
		<IssuerList<T>>::get().contains(firm)
	}

	/// Transfers the given `amount` of shares of the given `firm`, to the `to` AccountId
	fn transfer_share(from: T::AccountId, to: T::AccountId, firm: T::AccountId, amount_to_send: u64) -> Result {
		let shares_before_trans = Self::owned_shares((from.clone(), firm.clone()));
		ensure!(shares_before_trans >= amount_to_send, "[Error]you do not own enough shares so send");
		ensure!(Self::issuer_list().contains(&firm), "[Error]the firm does not exists");

		let shares_subbed = shares_before_trans.checked_sub(amount_to_send)
			.ok_or("[Error]underflow while subtracting shares")?;

		let shares_added = Self::owned_shares((to.clone(), firm.clone())).checked_add(amount_to_send)
			.ok_or("[Error]overflow while adding shares")?;

		// update the senders share amount
		<OwnedShares<T>>::insert((from.clone(), firm.clone()), shares_subbed);
		// update the receiver's amount
		<OwnedShares<T>>::insert((to.clone(), firm.clone()), shares_added);

		Self::deposit_event(RawEvent::TransferredShares(from, to, firm, amount_to_send));

		Ok(())
	}

	/// Generates and returns a random hash. This will mutate the Nonce storage value
	fn generate_hash(sender: T::AccountId) -> T::Hash {
		let nonce = Self::nonce();
		<Nonce<T>>::put(nonce + 1);
		(<system::Module<T>>::random_seed(), &sender, nonce).using_encoded(<T as system::Trait>::Hashing::hash)
	}

	// BConverts `u64` to `T::Balance` using the `As` trait, with `T: u64`, and then calling `sa`
    fn u64_to_balance(input: u64) -> T::Balance {
        <T::Balance as As<u64>>::sa(input)
    }

	/// Convert and return T::Balance into a u64
    fn balance_to_u64(input: T::Balance) -> u64 {
        input.as_()
    }

	fn add_sell_order_to_blockchain(
		from: T::AccountId,
		firm: T::AccountId, 
		owner: T::AccountId, 
		min_price: T::Balance, 
		amount: u64,){

		let new_hash = Self::generate_hash(from.clone());
		let make_sell_order = SellOrder {
			firm: firm.clone(),
			owner: from.clone(),
			min_price: min_price,
			amount: amount,
			order_id: new_hash.clone(),
		};
		// add the order to the blockchain storage list
		<SellOrdersList<T>>::mutate(&firm, |sell_order_list| sell_order_list.push(make_sell_order.clone()));
		Self::deposit_event(RawEvent::SubmittedSellOrder(owner, firm, amount, min_price, new_hash));
	}

	fn add_buy_order_to_blockchain(
		from: T::AccountId,
		firm: T::AccountId, 
		owner: T::AccountId, 
		max_price: T::Balance, 
		amount: u64,){

		let new_hash = Self::generate_hash(from.clone());
		let make_buy_order = BuyOrder {
			firm: firm.clone(),
			owner: from.clone(),
			max_price: max_price,
			amount: amount,
			order_id: new_hash.clone(),
		};
		// add the order to the blockchain storage list
		<BuyOrdersList<T>>::mutate(&firm, |buy_orders_list| buy_orders_list.push(make_buy_order.clone()));
		Self::deposit_event(RawEvent::SubmittedBuyOrder(owner, firm, amount, max_price, new_hash));
	}
}

decl_event!(
	pub enum Event<T> where 
	AccountId = <T as system::Trait>::AccountId,
	Balance = <T as balances::Trait>::Balance,
	Hash = <T as system::Trait>::Hash {
		GaveFirmIssueRight(AccountId, u64),
		RevokedFirmIssueRight(AccountId),
		MarketFrozen(AccountId, bool),
		AdjustedAuthorizedStock(AccountId, u64),
		IssuedShares(AccountId, u64),
		RetiredShares(AccountId, u64),
		// parameters are sender, to, issuer (firm), amount
		TransferredShares(AccountId, AccountId, AccountId, u64),
		// parameters are sender, issuer (firm), amount, min price
		SubmittedSellOrder(AccountId, AccountId, u64, Balance, Hash),
		// parameters are sender, issuer (firm), amount, max price
		SubmittedBuyOrder(AccountId, AccountId, u64, Balance, Hash),

	}
);

