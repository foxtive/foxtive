# Foxtive Changelog
Foxtive changelog file 

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