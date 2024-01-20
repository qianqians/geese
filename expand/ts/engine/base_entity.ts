/*
 * base_entity.ts
 * qianqians
 * 2023/10/5
 */

export class base_entity {
    public EntityType : string;
    public EntityID : string;

    public constructor(entity_type:string, entity_id:string) {
        this.EntityType = entity_type;
        this.EntityID = entity_id;
    }
}