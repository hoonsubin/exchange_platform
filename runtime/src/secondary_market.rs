//     Blockchain Stock Exchange Platform Secondary Market
//     This runtime module will attempt to emulate the traditional secondary market
//     in stock exchange market

//     Copyright (C) 2019  Hoon KIM

//     This program is free software: you can redistribute it and/or modify
//     it under the terms of the GNU General Public License as published by
//     the Free Software Foundation, either version 3 of the License, or
//     (at your option) any later version.

//     This program is distributed in the hope that it will be useful,
//     but WITHOUT ANY WARRANTY; without even the implied warranty of
//     MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//     GNU General Public License for more details.

//     You should have received a copy of the GNU General Public License
//     along with this program.  If not, see <https://www.gnu.org/licenses/>.

use parity_codec::{Decode, Encode};
use rstd::prelude::*;
use runtime_io::{self};
use runtime_primitives::traits::{As, CheckedMul, Hash};
use support::{
	decl_event, decl_module, decl_storage,
	dispatch::Result,
	ensure,
	traits::{Currency, ReservableCurrency},
	StorageMap, StorageValue,
};
use system::ensure_signed;

#[derive(Encode, Decode, Default, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct BuyOrder<AccountId, Balance, Hash, BlockNumber> {
	firm: AccountId,
	owner: AccountId,
	max_price: Balance,
	amount: u64,
	order_id: Hash,
	expire: BlockNumber,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct SellOrder<AccountId, Balance, Hash, BlockNumber> {
	firm: AccountId,
	owner: AccountId,
	min_price: Balance,
	amount: u64,
	order_id: Hash,
	expire: BlockNumber,
}

pub trait Trait: system::Trait + sudo::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
	type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;
}

type BalanceOf<T> = <<T as Trait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

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

		/// The number of shares of the company for the user that is locked and cannot be touched
		/// (parameters: Holder, firm)
		LockedShares get(locked_shares): map (T::AccountId, T::AccountId) => u64;

		/// The last traded price of the given company's share. This is used to track market price
		LastBidPrice get(last_bid_price): map T::AccountId => BalanceOf<T>;

		/// The maximum shares the given company can issue
		AuthorizedShares get(authorized_shares): map T::AccountId => u64;

		/// The market freeze state. Making this true will stop all further exchange
		CloseMarket get(market_closed): bool = false;

		/// The list of sell orders for a particular company's account
		SellOrdersList get(sell_orders_list): map T::AccountId => Vec<SellOrder<T::AccountId, BalanceOf<T>, T::Hash, T::BlockNumber>>;

		/// The list of buy orders for a particular company's account
		BuyOrdersList get(buy_orders_list): map T::AccountId => Vec<BuyOrder<T::AccountId, BalanceOf<T>, T::Hash, T::BlockNumber>>;

		/// List of sell order that are expiring in the given block number
		SellOrdersExpiring get(sell_order_expiring): map T::BlockNumber => Vec<SellOrder<T::AccountId, BalanceOf<T>, T::Hash, T::BlockNumber>>;

		/// List of buy order that are expiring in the given block number
		BuyOrdersExpiring get(buy_order_expiring): map T::BlockNumber => Vec<BuyOrder<T::AccountId, BalanceOf<T>, T::Hash, T::BlockNumber>>;

		/// A nonce value used for generating random values
		Nonce get(nonce): u64 = 0;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;

		// this function is only for quick testing
		pub fn unreserve_currency(origin, amount: BalanceOf<T>){
			let sender = ensure_signed(origin)?;

			T::Currency::unreserve(&sender, amount);
		}

		/// Searches the current buy orders and see if there is a price match for the transaction.
		/// If there are no buy orders, this will create a new sell order which will be checked by the
		/// other traders who call place_buy_order function.
		pub fn place_sell_order(origin, firm: T::AccountId, amount: u64, min_price: BalanceOf<T>, until: T::BlockNumber) -> Result {
			let sender = ensure_signed(origin)?;
			// all the other checks will be done within other functions, so we only check this
			ensure!(!Self::market_closed(), "[Error]the market is frozen right now");
			ensure!(until > <system::Module<T>>::block_number(), "[Error]you cannot set the expiration block lower than the current block");
			ensure!(Self::balance_to_u64(min_price.clone()) > 0, "[Error]you cannot sell for 0");
			ensure!(Self::owned_shares((firm.clone(), sender.clone())) >= amount,
				"[Error]you do not own enough shares of this company");
			// get the entire buy orders from the blockchain
			// a mutable copy of the list that we will be making changes to
			let mut temp_buy_orders_list = Self::buy_orders_list(&firm);

			// this is also a clone of the master list, but we will not make changes to this one
			let mut new_order_list = temp_buy_orders_list.clone();

			// make a new list of all the orders that are not going to be mutated
			new_order_list.retain(|x| x.max_price < min_price);

			// only get the orders where the max price is lower or equal to the min price, and is not expired
			temp_buy_orders_list.retain(|x| x.max_price >= min_price && x.amount > 0);

			// used to track how much shares the caller needs to sell
			let mut remaining_shares = amount;

			// check if the number orders are greater than 0
			if !temp_buy_orders_list.is_empty() {

				// sort the vector from higher max_price to low
				//todo: the ordering of the list can be enhanced to better satisfy both parties
				temp_buy_orders_list.sort_by(|a, b| b.max_price.cmp(&a.max_price));

				// we are cloning the master list because we will be making changes to it during the loop
				for order in temp_buy_orders_list.clone() {

					if remaining_shares == 0 {
						// break the loop if the caller sold all the shares
						break;
					}
					// check if the order of the share is enough
					else if order.amount <= remaining_shares {
						let total_price = order.max_price.checked_mul(&Self::u64_to_balance(order.amount))
							.ok_or("[Error]overflow in calculating total price")?;

						// unreserve the balance
						T::Currency::unreserve(&order.owner, total_price);

						// then send all the buyer's requested amount to the buyer
						// pattern match so we can move on to the next order when there is an error
						match Self::execute_order(sender.clone(), order.owner.clone(), firm.clone(), order.amount, order.max_price) {
							Err(_e) => continue,
							Ok(_v) => {
								remaining_shares = remaining_shares.checked_sub(order.amount)
									.ok_or("[Error]underflow while subtracting new shares")?;
								// remove the current order from the master list once the transaction is done
								temp_buy_orders_list.retain(|x| x.order_id != order.order_id);

								<BuyOrdersExpiring<T>>::mutate(&until, |buy_orders_list| {
									buy_orders_list.retain(|x| x.order_id != order.order_id)
								});

								runtime_io::print("[Debug]buy order fully satisfied");
							},
						}
					}
					// if the amount of buy is greater than the amount to sell
					else if order.amount > remaining_shares {
						
						let total_reserve = order.max_price.checked_mul(&Self::u64_to_balance(order.amount))
							.ok_or("[Error]overflow in calculating total price")?;
						
						// unreserve the entire balance from the order
						T::Currency::unreserve(&order.owner, total_reserve);

						// pattern match so we can move on to the next order when there is an error
						match Self::execute_order(sender.clone(), order.owner.clone(), firm.clone(), remaining_shares, order.max_price) {
							Err(_e) => continue,
							Ok(_v) => {

								let shares_selling = order.amount.checked_sub(remaining_shares)
									.ok_or("[Error]underflow while calculating left shares")?;
								
								let total_price = order.max_price.checked_mul(&Self::u64_to_balance(shares_selling))
									.ok_or("[Error]overflow in calculating total price")?;
								
								// remove the current order from the master list once the transaction is done
								temp_buy_orders_list.retain(|x| x.order_id != order.order_id);

								<BuyOrdersExpiring<T>>::mutate(&until, |buy_orders_list| {
									buy_orders_list.retain(|x| x.order_id != order.order_id)
								});

								let new_buy_order = BuyOrder{
									firm: order.firm,
									owner: order.owner.clone(),
									max_price: order.max_price,
									amount: shares_selling,
									order_id: Self::generate_hash(sender.clone()),
									expire: order.expire,
								};

								remaining_shares = 0;

								// reserve the cash
								T::Currency::reserve(&order.owner, total_price)
									.map_err(|_| "[Error]locker can't afford to lock the amount requested")?;

								// add the adjusted order to the list
								temp_buy_orders_list.push(new_buy_order.clone());

								// add this mutated order to the expiring list
								<BuyOrdersExpiring<T>>::mutate(&until, |buy_orders_list| {
									buy_orders_list.push(new_buy_order.clone())
								});

								runtime_io::print("[Debug]sell order partially satisfied (made new buy order)");

								// break out of the for loop once the caller sold all the shares
								break;
							},
						}
					}
				}
				// combine the order list that wasn't touched, and the adjusted ones
				new_order_list.append(&mut temp_buy_orders_list);

				// replace the entire list with the new one
				<BuyOrdersList<T>>::insert(&firm, new_order_list);
			}

			// if there are no existing buy orders with the right price in the market
			if remaining_shares > 0 {
				// create a new sell order so later buyers can check it
				// this function will lock the shares for us
				Self::add_sell_order_to_blockchain(sender.clone(),
				firm.clone(),
				sender.clone(),
				min_price,
				remaining_shares,
				until)?;
			}
			Ok(())
		}

		/// Searches the current sell orders and see if there is a price match for the transaction.
		/// If there are no sell orders, this will create a new buy order which will be checked by the
		/// other traders who call place_sell_order function.
		pub fn place_buy_order(origin, firm: T::AccountId, amount: u64, max_price: BalanceOf<T>, until: T::BlockNumber) -> Result {
			let sender = ensure_signed(origin)?;
			// all the other checks will be done within other functions, so we only check this
			ensure!(!Self::market_closed(), "[Error]the market is frozen right now");
			ensure!(until > <system::Module<T>>::block_number(), "[Error]you cannot set the expiration block lower than the current block");
			ensure!(Self::balance_to_u64(max_price.clone()) > 0, "[Error]you cannot buy for 0");

			// check if the caller has enough balance
			let total_price = max_price.checked_mul(&Self::u64_to_balance(amount.clone()))
				.ok_or("[Error]overflow in calculating total price")?;
			ensure!(T::Currency::free_balance(&sender) >= total_price,
				"[Error]you don't have enough free balance for this trade");
			// create two copies of the master list.
			let mut temp_sell_list = Self::sell_orders_list(&firm);
			let mut new_sell_list = temp_sell_list.clone();

			// make a new list of all the orders that are not going to be mutated
			new_sell_list.retain(|x| x.min_price > max_price);

			// only get the orders where the min price is lower or equal to the max price
			// we will be making adjustments to this list to update the global list
			temp_sell_list.retain(|x| x.min_price <= max_price && x.amount > 0);

			// track the shares to buy
			let mut remaining_shares_to_buy = amount;

			// check if the number of valid orders are greater than 0
			if !temp_sell_list.is_empty() {
				// sort the vector from lowest min_price to high
				//todo: the ordering of the list can be enhanced to better satisfy both parties
				temp_sell_list.sort_by(|a, b| a.min_price.cmp(&b.min_price));

				for order in temp_sell_list.clone(){
					// break the loop if the caller bought all the shares
					if remaining_shares_to_buy == 0 {
						break;
					}
					else if order.amount <= remaining_shares_to_buy {
						// first unlock the shares before transferring them
						match Self::unlock_shares(order.owner.clone(), order.firm.clone(), order.amount) {
							// continue on to the next loop when there is an error
							Err(_e) => continue,
							Ok(_v) => {
								// transfer the shares to the caller
								match Self::execute_order(order.owner.clone(), sender.clone(), firm.clone(), order.amount, order.min_price) {
									Err(_e) => continue,
									Ok(_v) => {
										// subtract remaining shares to buy after the transfer is over
										remaining_shares_to_buy = remaining_shares_to_buy.checked_sub(order.amount)
											.ok_or("[Error]underflow while subtracting new shares")?;

										// remove the current order from the master list once the transaction is done
										temp_sell_list.retain(|x| x.order_id != order.order_id);

										<SellOrdersExpiring<T>>::mutate(&until, |sell_orders_list| {
											sell_orders_list.retain(|x| x.order_id != order.order_id)
										});

										runtime_io::print("[Debug]sell order fully satisfied");
									},
								}
							},
						}
					}
					// if the amount of buy is greater than the amount to sell
					else if order.amount > remaining_shares_to_buy {

						let share_left = order.amount.checked_sub(remaining_shares_to_buy)
							.ok_or("[Error]underflow during calculation of total shares")?;
						match Self::unlock_shares(order.owner.clone(), order.firm.clone(), remaining_shares_to_buy) {
							// continue on to the next loop when there is an error
							Err(_e) => continue,
							Ok(_v) => {
								// transfer the shares to the caller
								match Self::execute_order(order.owner.clone(), sender.clone(), firm.clone(), remaining_shares_to_buy, order.min_price) {
									Err(_e) => continue,
									Ok(_v) => {

										// remove the current order from the master list once the transaction is done
										temp_sell_list.retain(|x| x.order_id != order.order_id);

										<SellOrdersExpiring<T>>::mutate(&until, |sell_orders_list| {
											sell_orders_list.retain(|x| x.order_id != order.order_id)
										});

										// make a new sell order with the subtracted amount
										let adjusted_sell_order = SellOrder {
											firm: order.firm,
											owner: order.owner,
											min_price: order.min_price,
											amount: share_left,
											order_id: Self::generate_hash(sender.clone()),
											expire: order.expire,
										};

										remaining_shares_to_buy = 0;

										// push (add to the last index) the newly adjusted sell order to the master list
										temp_sell_list.push(adjusted_sell_order.clone());

										// add the mutated sell order to the expiring list
										<SellOrdersExpiring<T>>::mutate(&until, |sell_orders_list| {
											sell_orders_list.push(adjusted_sell_order)
										});

										runtime_io::print("[Debug]buy order partially satisfied (made new sell order)");
										
										// break out of the for loop to combine the adjusted list
										break;
									},
								}
							},
						}
					}
				}
				// combine the order list that wasn't touched, and the adjusted ones
				new_sell_list.append(&mut temp_sell_list);

				// replace the entire list with the new one
				<SellOrdersList<T>>::insert(&firm, new_sell_list);
			}

			// if there are no good orders in the market
			if remaining_shares_to_buy > 0 {
				// create a new sell order so later buyers can check it
				// this function also handles the currency lock as well
				Self::add_buy_order_to_blockchain(sender.clone(),
				firm.clone(),
				sender.clone(),
				max_price,
				remaining_shares_to_buy,
				until)?;
			}
			Ok(())
		}

		/// Give a given company the right to issue shares with the given authorized shares.
		/// You can only change the number of authorized shares through the change_authorized_shares function.
		pub fn give_issue_rights(origin, firm: T::AccountId, authorized_shares: u64) -> Result {
			let sender = ensure_signed(origin)?;
			ensure!(sender.clone() == <sudo::Module<T>>::key(), "[Error]the caller must have sudo key to give rights");

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
			let sender = ensure_signed(origin)?;
			ensure!(sender.clone() == <sudo::Module<T>>::key(), "[Error]the caller must have sudo key to give rights");
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
			let sender = ensure_signed(origin)?;
			ensure!(sender.clone() == <sudo::Module<T>>::key(),
				"[Error]the caller must have sudo key to give rights");
			ensure!(sender != firm.clone(), "[Error]you cannot change your own issue limit");
			ensure!(<IsAllowedIssue<T>>::get(&firm), "[Error]the firm is not allowed to issue shares");
			ensure!(new_limit > Self::floating_shares(&firm),
				"[Error]the firm cannot limit shares less than the already issued amount");

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

			let new_shares_outstanding = old_shares_outstanding.checked_add(amount)
				.ok_or("[Error]overflowing when issuing new shares")?;

			ensure!(new_shares_outstanding < Self::authorized_shares(&sender),
				"[Error]already issued the maximum amount of shares");

			// add the firm to the list if it is not in there
			if !Self::is_firm(&sender) {
				<IssuerList<T>>::mutate(|account| account.push(sender.clone()));
			}

			// add currently owned shares with the additional shares issued
			let owned_shares_total = Self::owned_shares((sender.clone(), sender.clone())).checked_add(amount)
				.ok_or("[Error]overflow when adding total owned shares")?;
			
			<OwnedShares<T>>::insert((sender.clone(), sender.clone()), owned_shares_total);
			<FloatingShares<T>>::insert(&sender, new_shares_outstanding);

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
			
			// if the calling firm is in the firm list, and has no floating shares
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

		/// Changes the `CloseMarket` storage value to true. This will prevent any trading from happening.
		/// Only accounts with sudo keys can call this
		pub fn close_market(origin) -> Result {
			let sender = ensure_signed(origin)?;
			ensure!(sender.clone() == <sudo::Module<T>>::key(), "[Error]the caller must have sudo key to give rights");

			ensure!(!Self::market_closed(), "[Error]the market is already frozen");

			<CloseMarket<T>>::put(true);
			Self::deposit_event(RawEvent::MarketFrozen(sender, true));

			Ok(())
		}

		/// Changes the `CloseMarket` storage value to false. This will allow all trades to happen.
		/// Only accounts with sudo keys can call this
		pub fn open_market(origin) -> Result {
			let sender = ensure_signed(origin)?;
			ensure!(sender.clone() == <sudo::Module<T>>::key(), "[Error]the caller must have sudo key to give rights");

			ensure!(Self::market_closed(), "[Error]the market is already not frozen");

			<CloseMarket<T>>::put(false);
			Self::deposit_event(RawEvent::MarketFrozen(sender, false));

			Ok(())
		}

		// run after every runtime function
		fn on_finalize() {
			let current_block = <system::Module<T>>::block_number();

			let buy_orders_expiring = Self::buy_order_expiring(current_block);
			let sell_orders_expiring = Self::sell_order_expiring(current_block);
			let issuers = Self::issuer_list();

			for firm in issuers {
				if !buy_orders_expiring.is_empty() {
					// get all the buy orders from the current issuer
					let mut buy_order_vec = Self::buy_orders_list(&firm);

					// only get the orders that are expiring this block
					let mut expiring_orders = buy_order_vec.clone();
					expiring_orders.retain(|x| x.expire <= current_block);

					if !expiring_orders.is_empty() {
						for order in expiring_orders {
							let total_price = order.max_price.checked_mul(&Self::u64_to_balance(order.amount))
								.expect("the data type for the currency is u128 and the type for amount is u64");
						
							// unreserve the balance for this order
							T::Currency::unreserve(&order.owner, total_price);
							Self::deposit_event(RawEvent::BuyOrderExpired(order.owner.clone(), order.firm.clone(), total_price, order.amount));
							runtime_io::print("[Debug]buy order expired");
						}
	
						// get the orders expiring buying this firm's share
						buy_order_vec.retain(|x| x.expire > current_block);
	
						<BuyOrdersList<T>>::insert(&firm, &buy_order_vec);
					}
				}

				if !sell_orders_expiring.is_empty() {
					let mut sell_order_vec = Self::sell_orders_list(&firm);

					// only get the orders that are expiring this block
					let mut expiring_orders = sell_order_vec.clone();
					expiring_orders.retain(|x| x.expire <= current_block);

					if !expiring_orders.is_empty() {
						for order in expiring_orders {
							Self::unlock_shares(order.owner.clone(), order.firm.clone(), order.amount)
								.expect("I really hope this works lol");

							let total_price = order.min_price.checked_mul(&Self::u64_to_balance(order.amount))
								.expect("the data type for the currency is u128 and the type for amount is u64");
							
							Self::deposit_event(RawEvent::SellOrderExpired(order.owner.clone(), order.firm.clone(), total_price, order.amount));
							runtime_io::print("[Debug]sell order expired");
						}
	
						sell_order_vec.retain(|x| x.expire > current_block);
						// update the blockchain storage list
						<SellOrdersList<T>>::insert(&firm, &sell_order_vec);
					}
					
				}

				// break the loop with there are no expiring orders in this list
				if sell_orders_expiring.is_empty() && buy_orders_expiring.is_empty() {
					break;
				}
			}
			
		}
	}
}

// private functions for the runtime module. This is not exposed to the RPC
impl<T: Trait> Module<T> {
	/// checks if the given AccountId has share issue rights
	fn is_firm(firm: &T::AccountId) -> bool {
		<IssuerList<T>>::get().contains(firm)
	}

	/// Lock the given `amount` of shares that the `owner` has for the `firm`.
	/// Locking the shares ensures that those shares will not be spent until it is unlocked
	fn lock_shares(owner: T::AccountId, firm: T::AccountId, amount: u64) -> Result {
		// get the currently owned amount
		let owned = Self::owned_shares((owner.clone(), firm.clone()));

		ensure!(
			owned >= amount,
			"[Error]the account does not hold enough shares"
		);
		// check how much will be left after the lock
		let left_shares = owned
			.checked_sub(amount)
			.ok_or("[Error]underflow while calculating shares after lock")?;

		// insert the shares to the lock
		<LockedShares<T>>::insert((owner.clone(), firm.clone()), amount);
		// insert the shares left back to the owner
		<OwnedShares<T>>::insert((owner.clone(), firm.clone()), left_shares);

		Self::deposit_event(RawEvent::LockedShares(owner, firm, amount));

		Ok(())
	}

	/// Unlock the given `amount` of shares that the `owner` has for the `firm`.
	/// Unlocking shares will transfer the locked shares to the owned shares
	fn unlock_shares(owner: T::AccountId, firm: T::AccountId, amount: u64) -> Result {
		let locked = Self::locked_shares((owner.clone(), firm.clone()));
		ensure!(
			locked >= amount,
			"[Error]cannot unlock more than what is locked"
		);

		// locked - amount
		let subbed_locked = locked
			.checked_sub(amount)
			.ok_or("[Error]underflow while calculating shares after unlock")?;
		// amount + currently owned share
		let total_shares = amount
			.checked_add(Self::owned_shares((owner.clone(), firm.clone())))
			.ok_or("[Error]overflow while calculating total shares after unlock")?;
		<OwnedShares<T>>::insert((owner.clone(), firm.clone()), total_shares);
		<LockedShares<T>>::insert((owner.clone(), firm.clone()), subbed_locked);

		Self::deposit_event(RawEvent::UnlockedShares(owner, firm, amount));

		Ok(())
	}

	/// Transfers the given `amount` of shares of the given `firm`, to the `to` AccountId.
	/// And the `to` account will send the `price_per_share` to the `from` account.
	/// This function handles safe maths, transfer of shares, transfer of coins, and updates last bid price storage
	fn execute_order(
		from: T::AccountId,
		to: T::AccountId,
		firm: T::AccountId,
		amount_to_send: u64,
		price_per_share: BalanceOf<T>,
	) -> Result {
		// the owned shares for the sender
		let shares_before_trans = Self::owned_shares((from.clone(), firm.clone()));
		// calculate the total price for this transfer
		let total_price = price_per_share
			.checked_mul(&Self::u64_to_balance(amount_to_send))
			.ok_or("[Error]overflow in calculating total price")?;

		let shares_subbed = shares_before_trans
			.checked_sub(amount_to_send)
			.ok_or("[Error]underflow while subtracting shares. You don't own enough shares")?;

		// this part shows a bug where if the `to` and `from` is the same, the share number doubles
		let mut shares_added = Self::owned_shares((to.clone(), firm.clone()))
			.checked_add(amount_to_send)
			.ok_or("[Error]overflow while adding shares")?;
		//* this is a very hacky, temporary solution to address the share dup bug
		//* try finding a better solution if possible
		if &from == &to {
			// if the caller is sending the shares to itself
			// we don;t change the value of the share
			shares_added = shares_before_trans;
		}
		// the account receiving the share will send the money to the person sending it
		T::Currency::transfer(&to, &from, total_price)?;

		// update the senders share amount
		<OwnedShares<T>>::insert((from.clone(), firm.clone()), shares_subbed);
		// update the receiver's amount
		<OwnedShares<T>>::insert((to.clone(), firm.clone()), shares_added);
		// update the last bid price for this share
		<LastBidPrice<T>>::insert(firm.clone(), price_per_share);
		runtime_io::print("[Debug]transferred shares");

		Self::deposit_event(RawEvent::TransferredShares(from, to, firm, amount_to_send));
		runtime_io::print("[Debug]executed order");

		Ok(())
	}

	/// Generates and returns a random hash. This will mutate the Nonce storage value
	fn generate_hash(sender: T::AccountId) -> T::Hash {
		let nonce = Self::nonce();
		<Nonce<T>>::put(nonce + 1);
		(<system::Module<T>>::random_seed(), &sender, nonce)
			.using_encoded(<T as system::Trait>::Hashing::hash)
	}

	// BConverts `u64` to `BalanceOf<T>` using the `As` trait, with `T: u64`, and then calling `sa`
	fn u64_to_balance(input: u64) -> BalanceOf<T> {
		<BalanceOf<T> as As<u64>>::sa(input)
	}

	/// Convert and return BalanceOf<T> into a u64
	fn balance_to_u64(input: BalanceOf<T>) -> u64 {
		input.as_()
	}

	/// Adds a sell order to the blockchain storage list
	/// This will also automatically lock the `amount` of shares
	fn add_sell_order_to_blockchain(
		from: T::AccountId,
		firm: T::AccountId,
		owner: T::AccountId,
		min_price: BalanceOf<T>,
		amount: u64,
		until: T::BlockNumber,
	) -> Result {
		ensure!(
			Self::issuer_list().contains(&firm),
			"[Error]the firm does not exists"
		);

		let new_hash = Self::generate_hash(from.clone());
		let make_sell_order = SellOrder {
			firm: firm.clone(),
			owner: from.clone(),
			min_price: min_price,
			amount: amount,
			order_id: new_hash.clone(),
			expire: until,
		};
		// add the order to the blockchain storage list
		<SellOrdersList<T>>::mutate(&firm, |sell_orders_list| {
			sell_orders_list.push(make_sell_order.clone())
		});

		<SellOrdersExpiring<T>>::mutate(&until, |sell_orders_list| {
			sell_orders_list.push(make_sell_order.clone())
		});

		// lock the shares
		Self::lock_shares(owner.clone(), firm.clone(), amount)?;
		Self::deposit_event(RawEvent::SubmittedSellOrder(
			owner, firm, amount, min_price, new_hash,
		));

		runtime_io::print("[Debug]placed sell order");

		Ok(())
	}

	/// Adds a buy order to the blockchain storage list
	fn add_buy_order_to_blockchain(
		from: T::AccountId,
		firm: T::AccountId,
		owner: T::AccountId,
		max_price: BalanceOf<T>,
		amount: u64,
		until: T::BlockNumber,
	) -> Result {
		ensure!(
			Self::issuer_list().contains(&firm),
			"[Error]the firm does not exists"
		);

		let new_hash = Self::generate_hash(from.clone());
		let make_buy_order = BuyOrder {
			firm: firm.clone(),
			owner: from.clone(),
			max_price: max_price,
			amount: amount,
			order_id: new_hash.clone(),
			expire: until,
		};

		let total_price = max_price
			.checked_mul(&Self::u64_to_balance(amount))
			.ok_or("[Error]overflow in calculating total price")?;

		// reserve the balance
		T::Currency::reserve(&owner, total_price)
			.map_err(|_| "locker can't afford to lock the amount requested")?;

		// add the order to the blockchain storage list
		<BuyOrdersList<T>>::mutate(&firm, |buy_orders_list| {
			buy_orders_list.push(make_buy_order.clone())
		});

		<BuyOrdersExpiring<T>>::mutate(&until, |buy_orders_list| {
			buy_orders_list.push(make_buy_order.clone())
		});

		Self::deposit_event(RawEvent::SubmittedBuyOrder(
			owner, firm, amount, max_price, new_hash,
		));

		runtime_io::print("[Debug]placed buy order");

		Ok(())
	}
}

decl_event!(
	pub enum Event<T>
	where
		AccountId = <T as system::Trait>::AccountId,
		Balance = BalanceOf<T>,
		Hash = <T as system::Trait>::Hash,
	{
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
		// parameters are owner, issuer (firm), amount
		LockedShares(AccountId, AccountId, u64),
		// parameters are owner, issuer (firm), amount
		UnlockedShares(AccountId, AccountId, u64),
		// parameters are owner, issuer (firm), total price, amount
		BuyOrderExpired(AccountId, AccountId, Balance, u64),
		// parameters are owner, issuer (firm), total price, amount
		SellOrderExpired(AccountId, AccountId, Balance, u64),
	}
);
