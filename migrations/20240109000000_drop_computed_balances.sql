-- Remove computed daily balance rows; these are now derived on the fly
-- from reported balances + transaction sums.
DELETE FROM daily_balances WHERE source = 'computed';
