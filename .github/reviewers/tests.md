---
model: sonnet
---

Judge the tests the diff adds or changes, and the tests it should have.

- `test/*.test.js` (Node's test runner) covers the pure logic in `src/`;
  `test/e2e/` (Playwright, phone size) covers what the user does;
  `test/visual/` only captures screenshots. A change to behaviour comes
  with a test at the level where the behaviour lives, and a bug fix with a
  test that fails without the fix.
- Is the expected result right? Read each test as the user would and say
  whether they would expect that outcome. A test that pins a wrong result
  pins a bug, whatever its name says.
- A changed expectation in an existing test: does the PR explain it, or does
  it hide a regression? Say which.
- A removed or moved test: is what it checked still checked somewhere?
- Each test checks one clear thing, and its name says what. Browser tests
  wait for a condition (`expect`, `expect.poll`), never a fixed sleep for
  something the app does, and find steps by their exact title (the `step()`
  helper), not by text that other rows also contain.

For each finding, say what the test should check instead.
