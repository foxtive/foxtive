# Foxtive
Foxtive combines creativity with technical expertise to build impactful software that drives results.

## Running Tests

### Prerequisites
- Ensure Redis is running and accessible
- Set up required environment variables

#### Environment Variables
- `RUST_TEST_THREADS=1`: Forces tests to run sequentially instead of in parallel, which is important for tests that share resources (like Redis connections)
- `TEST_REDIS_DSN`: Connection string for the Redis instance used in testing

### Running Tests
To run the test suite, use the following command:

### Running Individual Tests
To run a specific test, you can add the test name to the command:


### Running Tests with Logging
To enable debug logging during tests, you can set the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug RUST_TEST_THREADS=1 TEST_REDIS_DSN="redis://default:Pass.1234@127.0.0.1:8379" cargo test --all-features --all-targets
```
