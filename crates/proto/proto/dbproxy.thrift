struct reg_hub_event {
    1:string hub_name
}

struct get_guid_event {
    1:string db,
    2:string collection,
    3:string callback_id
}

struct create_object_event {
    1:string db,
    2:string collection,
    3:string callback_id,
    4:binary object_info
}

struct update_object_event {
    1:string db,
    2:string collection,
    3:string callback_id,
    4:binary query_info,
    5:binary updata_info,
    6:bool _upsert
}

struct find_and_modify_event {
    1:string db,
    2:string collection,
    3:string callback_id,
    4:binary query_info,
    5:binary updata_info,
    6:bool _new,
    7:bool _upsert
}

struct remove_object_event {
    1:string db,
    2:string collection,
    3:string callback_id,
    4:binary query_info
}

struct get_object_info_event {
    1:string db,
    2:string collection,
    3:string callback_id,
    4:binary query_info,
    5:i32 skip,
    6:i32 limit,
    7:string sort,
    8:bool ascending
}

struct get_object_count_event {
    1:string db,
    2:string collection,
    3:string callback_id,
    4:binary query_info
}

union db_event {
    1:reg_hub_event reg_hub,
    2:get_guid_event get_guid,
    3:create_object_event create_object,
    4:update_object_event update_object,
    5:find_and_modify_event find_and_modify,
    6:remove_object_event remove_object,
    7:get_object_info_event get_object_info,
    8:get_object_count_event get_object_count
}
