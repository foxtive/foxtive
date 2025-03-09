use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct IdsVecDto {
    pub ids: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct IdUuidDto {
    pub id: Uuid,
}
