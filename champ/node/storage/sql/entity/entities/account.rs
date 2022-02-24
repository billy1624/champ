use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "accounts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub public_key: Vec<u8>,

    #[sea_orm(indexed)]
    pub account_id_v1: Vec<u8>,
    pub latest_block: Vec<u8>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::block::Entity")]
    Block,

    #[sea_orm(
        belongs_to = "super::block::Entity",
        from = "Column::LatestBlock",
        to = "super::block::Column::BlockId"
    )]
    LatestBlock,
}

impl Related<super::block::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LatestBlock.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
