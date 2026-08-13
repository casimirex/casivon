-- Connect a login to the employee it belongs to.
--
-- `employees.user_id` has existed since `007_create_hr.sql`, and
-- `EmployeeRepository::find_by_user_id` has existed with a working
-- implementation for just as long — called from nowhere. Somebody built the
-- link and never wired it up, which is why every HR endpoint has been happy to
-- serve any signed-in user anyone else's leave, expenses and balances, and to
-- let them file claims in another person's name.
--
-- Scoping those endpoints to the caller needs this column populated. Nothing
-- populates it: the employee form cannot set it, so on any existing database
-- every employee is unlinked and a strict rule would empty the module for
-- everybody at once.

-- --------------------------------------------------------------- the backfill
--
-- Matched on the address, which is the only thing the two tables share.
--
-- Only where it is unambiguous on **both** sides: exactly one employee and
-- exactly one user with that address. A duplicate on either side is left alone
-- rather than guessed at, because guessing wrong hands somebody another
-- person's records — precisely the failure this change exists to prevent.
UPDATE employees e
SET user_id = u.id
FROM users u
WHERE u.email = e.email
  AND e.user_id IS NULL
  AND (SELECT count(*) FROM employees x WHERE x.email = e.email) = 1
  AND (SELECT count(*) FROM users y WHERE y.email = e.email) = 1;

-- Employees left unlinked are the ordinary case, not a failure: plenty of staff
-- have no account in the system. They simply have no self-service records, and
-- HR can link them from the employee form when they get a login.

-- ------------------------------------------------------- one login, one person
--
-- `find_by_user_id` returns an `Option` and has always implied at most one
-- employee per login. Nothing enforced it, so two employee rows could name the
-- same user and the lookup would return whichever the planner happened to
-- reach first — a coin toss deciding whose expenses you can see.
--
-- Partial, because unlinked employees are expected and many of them share the
-- NULL.
CREATE UNIQUE INDEX idx_employees_user_id
    ON employees (user_id)
    WHERE user_id IS NOT NULL;
