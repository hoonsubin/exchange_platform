# Substrate Stock Exchange Platform
 A stock exchange platform made with substrate for undergrad thesis.
 
## Limitations of the current build

 - It only supports sell/buy limit order. Market order is technically
   possible if there are enough traders and transactions in the market,
   but for now I only track the market value.
 - There is no order limit or order expiration now, but it is quite easy
   to implement that later. But am i gonna?! (yes eventually)

## Public Functions

 - Put sell/buy order
 - Give issue rights (substitute for IPO) (sudo)
 - Revoke issue rights (sudo)
 - Change authorized shares (sudo)
 - Issue shares
 - Retire shares
 - Freeze/unfreeze market (sudo)
(sudo means the caller must have sudo keys)
