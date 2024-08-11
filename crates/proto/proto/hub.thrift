include "common.thrift"

/*
 * ntf_client_request_login.
 */
struct client_request_login {
	1:string gate_name, 
	2:string gate_host,
	3:string conn_id,
	4:string sdk_uuid
}

/*
 * ntf_client_request_reconnect.
 */
struct client_request_reconnect {
	1:string gate_name,  
	2:string gate_host,
	3:string conn_id,
	4:string account_id,
	5:string token
}

/*
 * ntf_client_request_service.
 */
struct client_request_service {
	1:string service_name,
	2:string gate_name, 
	3:string gate_host,
	4:string conn_id
}

/*
 * gate notify client exist server, old msg send complete.
 */
struct transfer_msg_end {
	1:string conn_id,
	2:bool is_kick_off;
}

/*
 * gate notify transfer client conn
 */
struct transfer_entity_control {
	1:string entity_id,
	2:bool is_main,
	3:bool is_replace,
	4:string gate_name,
	5:string conn_id
}

/*
 * gate notify client kick off
 */
struct kick_off_client {
	1:string conn_id
}

/*
 * gate notify client disconnnect
 */
struct client_disconnnect {
	1:string conn_id
}

/*
 * client call rpc to hub.
 */
struct client_call_rpc {
	1:string conn_id,
	2:string entity_id,
	3:i64 msg_cb_id,
	4:common.msg message
}

/*
 * client callback rsp to hub.
 */
struct client_call_rsp {
	1:common.rpc_rsp rsp
}

/*
 * client callback err to hub.
 */
struct client_call_err {
	1:common.rpc_err err
}

/*
 * client send ntf to hub.
 */
struct client_call_ntf {
	1:string entity_id,
	2:common.msg message
}

/*
 * query hub service entity
 */
struct query_service_entity {
	1:string service_name
}

/*
 * ack hub query service entity
 */
struct create_service_entity {
	1:string service_name,
	2:string entity_id,
	3:string entity_type,
	4:binary argvs
}

/*
 * hub forward ntf_client_request_service.
 */
struct hub_forward_client_request_service {
	1:string service_name,
	2:string gate_name, 
	3:string gate_host,
	4:string conn_id,
}

/*
 * hub call rpc to hub.
 */
struct hub_call_hub_rpc {
	1:string entity_id,
	2:i64 msg_cb_id,
	3:common.msg message,
}

/*
 * hub callback rsp to hub.
 */
struct hub_call_hub_rsp {
	1:common.rpc_rsp rsp
}

/*
 * hub callback err to hub.
 */
struct hub_call_hub_err {
	1:common.rpc_err err
}

/*
 * hub call ntf to hub.
 */
struct hub_call_hub_ntf {
	1:string entity_id,
	2:common.msg message,
}

/*
 * hub ntf hub wait migrate entity.
 */
struct hub_call_hub_wait_migrate_entity {
	1:string entity_id,
}

/*
 * hub ntf hub migrate entity.
 */
struct hub_call_hub_migrate_entity {
	1:string service_name,
	2:string entity_id,
	3:string entity_type,
	4:binary argvs
}

/*
 * hub ntf hub migrate entity complete.
 */
struct hub_call_hub_migrate_entity_complete {
	1:string entity_id,
}

union hub_service {
	1:client_request_login client_request_login,
	2:client_request_reconnect client_request_reconnect,
	3:transfer_msg_end transfer_msg_end,
	4:transfer_entity_control transfer_entity_control,
	5:kick_off_client kick_off_client,
	6:client_disconnnect client_disconnnect,
	7:client_request_service client_request_service,
	8:client_call_rpc client_call_rpc,
	9:client_call_rsp client_call_rsp,
	10:client_call_err client_call_err,
	11:client_call_ntf client_call_ntf,
	12:common.reg_server reg_server,
	13:common.reg_server_callback reg_server_callback,
	14:query_service_entity query_entity,
	15:create_service_entity create_service_entity,
	16:hub_forward_client_request_service hub_forward_client_request_service,
	17:hub_call_hub_rpc hub_call_rpc,
	18:hub_call_hub_rsp hub_call_rsp,
	19:hub_call_hub_err hub_call_err,
	20:hub_call_hub_ntf hub_call_ntf,
	21:hub_call_hub_wait_migrate_entity wait_migrate_entity,
	22:hub_call_hub_migrate_entity migrate_entity,
	23:hub_call_hub_migrate_entity_complete migrate_entity_complete,
}

struct ack_get_guid {
	1:string callback_id,
	2:i64 guid
}

struct ack_create_object {
	1:string callback_id,
	2:bool result
}

struct ack_updata_object {
	1:string callback_id,
	2:bool result
}

struct ack_find_and_modify {
	1:string callback_id,
	2:binary object_info
}

struct ack_remove_object {
	1:string callback_id,
	2:bool result
}

struct ack_get_object_count {
	1:string callback_id,
	2:i32 count
}

struct ack_get_object_info {
	1:string callback_id,
	2:binary object_info
}

struct ack_get_object_info_end {
	1:string callback_id
}

union db_callback {
	1:ack_get_guid get_guid,
	2:ack_create_object create_object,
	3:ack_updata_object updata_object,
	4:ack_find_and_modify find_and_modify,
	5:ack_remove_object remove_object,
	6:ack_get_object_count get_object_count,
	7:ack_get_object_info get_object_info,
	8:ack_get_object_info_end get_object_info_end
}