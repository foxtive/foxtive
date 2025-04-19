# Foxtive Changelog
Foxtive changelog file 

### 0.7.0 (2024-04-02)
* fix(redis): now accepts value that accepts ToRedisArgs to avoid auto serializing values

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