---
model: sonnet
---

Judge the tests the diff adds or changes, and the tests it should have.

- `test/*.test.js` (Node's test runner) covers the pure logic in `src/`;
  `test/e2e/` (Playwright, phone size) covers what the user does;
  `test/visual/` only captures screenshots. A change to behaviour comes
  with a test at the level where the behaviour lives, and a bug fix with a
  test that fails without the fix.
- The domain rules exist twice while the app moves to the Rust core:
  `src/graph.js` and `core/` (crate `routines-core`). Their shared tests are
  data: `core/tests/vectors/*.json`, each case an input and the expected
  output, run by `test/vectors.test.js` against the JavaScript and by
  `core/tests/vectors.rs` against Rust. A change to a rule changes both
  implementations and comes with a vector case, not a test in one language
  only. `npm run vectors` writes the `expected` values from the JavaScript,
  so read every changed `expected` as a change in behaviour: is it the one
  the PR describes? A vector case that cannot fail (a rule the input never
  reaches, an empty result that every bug would also give) checks nothing.
- `core/tests/stored.rs` loads data captured from the app
  (`core/tests/fixtures/stored.json`). A change to what is stored updates
  that fixture and checks that older data still loads.
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
