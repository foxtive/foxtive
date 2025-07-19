# Foxtive Changelog
Foxtive changelog file 

### 0.14.0 (2024-06-19)
* feat(env): optionally use tracing logger for more advance logging capabilities
* bump(rust-argon2): to version 3.0

### 0.13.2 (2024-06-14)
* fix(file-ext): return ext without a dot (return "txt" instead of ".txt")
* bump(crates): to latest minor versions

### 0.13.1 (2024-06-12)
* feat(file-size): convenient methods for file size formatting
* bump(crates): to latest minor versions

### 0.13.0 (2024-06-05)
* bump(crates): redis, lapin & fancy-regex to their latest respective versions
* feat(file-ext): add file ext helper, this helper provides a convenient way to handle file ext easily
* feat(sanitizer): file name & HTML sanitizer, HTML sanitizer is gated by `html-sanitizier` feature

### 0.12.0 (2024-06-05)
* bump(edition): to 2024

### 0.11.0 (2024-06-05)
* bump(crates): to latest versions
* feat(env): add new AppMessage::MissingEnvironmentVariable(String, VarError)

### 0.10.0 (2024-05-23)
* feat(templating): BREAKING (expose template dir config)
* feat(rabbitmq): BREAKING (expose dsn & max pool size config)
* feat(redis): BREAKING (expose dsn & max pool size config)
* feat(database): BREAKING (expose dsn & max pool size config)

### 0.9.0 (2024-05-22)
* feat(cache): added 'forget_by_pattern' method to forget keys using pattern
* fix(templating): render now returns result instead of panicking
* feat(cache): added 'keys' and 'keys_by_pattern' methods

### 0.8.4 (2024-05-01)
* fix(serde_de_datetime): parse as string instead of &str

### 0.8.3 (2024-04-30)
* feat(string): extension trait to provide helper methods
* fix(reqwest): removed 'into_code()' & 'into_body()' and add 'into_parts()'

### 0.8.2 (2024-04-30)
* feat(reqwest): remove unnecessary result

### 0.8.1 (2024-04-30)
* feat(reqwest): error helper now has 'into_code()' & 'into_body()' to move the values out

### 0.8.0 (2024-04-20)
* feat(in-memory-cache): now support in memory driver using DashMap as an underlying storage

### 0.7.2 (2024-04-20)
* feat(hmac): constructor now accepts hashing function

### 0.7.1 (2024-04-20)
* fix(caching): drop the 'cache' feature

### 0.7.0 (2024-04-20)
* fix(redis): now accepts a value that accepts ToRedisArgs to avoid auto-serializing values
* docs(password): basic usage docs
* feat(string): add more utility funcs
* docs(hmac): basic usage docs
* refactor(hmac): support usage of multiple hash functions
* docs(base64): basic usage docs
* refactor(cache): introducing driver mechanism

### 0.6.5 (2024-04-02)
* feat(app-result): 'recover_from_async' to recover error from Error or AppResult<T>

### 0.6.4 (2024-03-31)
* feat(app-message): add 'is_success()', 'is_error()' & 'log()'

### 0.6.3 (2024-03-23)
* feat(pagination-result): added PaginationResultExt trait to provide .map_page_data()

### 0.6.2 (2024-03-22)
* fix(app-message): redirect status code is 302
* test(app-message): cover some cases
* feat(app-error): add AppErrorExt to provide .message() on error object

### 0.6.1 (2024-03-17)
* fix(app-message): .message() should return error message
* feat(workflow): tag version after release

### 0.6.0
* feat(app-message): impl Clone
* fix(crates): remove unused features

### 0.5.1
* fix(app-message): map other variants with status code

### 0.5.0
* feat(app-result): 'recover_from' to recover error from AppResult<T>

### 0.4.1
* added `AppResult<T>.map_app_msg(|m| m)`

### 0.4.0
* added database shareable ext
* move database traits to 'ext' folder
* impl From<crate::Error> for AppMessage 