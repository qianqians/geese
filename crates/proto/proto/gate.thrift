include "common.thrift"


/*
 * gate forward hub msg to client.
 * create remote entity in client.
 */
struct hub_call_client_create_remote_entity {
	1:list<string> conn_id,
	2:string main_conn_id,
	3:string entity_id,
	4:string entity_type,
	5:binary argvs
}
 
/*
 * hub command delete entity
 */
struct hub_call_client_delete_remote_entity {
	1:string entity_id
}

/*
 * gate forward hub msg to client.
 * refresh remote entity in client.
 */
struct hub_call_client_refresh_entity {
	1:string conn_id,
	2:bool is_main,
	3:string entity_id,
	4:string entity_type,
	5:binary argvs
}

/*
 * hub send rpc msg to client.
 */
struct hub_call_client_rpc {
	1:string entity_id,
	2:i64 msg_cb_id,
	3:common.msg message
}

/*
 * hub send rsp to client.
 */
struct hub_call_client_rsp {
	1:string conn_id,
	2:common.rpc_rsp rsp
}

/*
 * hub send err to client.
 */
struct hub_call_client_err {
	1:string conn_id,
	2:common.rpc_err err
}

/*
 * hub send ntf msg to client.
 */
struct hub_call_client_ntf {
	1:string conn_id,
	2:string entity_id,
	3:common.msg message
}

/*
 * hub send global msg to client.
 */
struct hub_call_client_global {
	1:common.msg message
}

/*
 * hub request kick off client.
 */
struct hub_call_kick_off_client {
	1:string conn_id,
	2:string prompt_info,
}

/*
 * hub ntf kick_off client complete
 */
struct hub_call_kick_off_client_complete {
	1:string conn_id
}

/*
 * hub request transfer client.
 */
struct hub_call_transfer_client {
	1:string conn_id,
	2:string prompt_info,
	3:string new_gate,
	4:string new_conn_id,
	5:bool is_replace,
}

/*
 * hub ntf transfer client complete
 */
struct hub_call_transfer_entity_complete {
	1:string conn_id,
	2:string entity_id,
}

union gate_hub_service {
	1:common.reg_server reg_server,
	2:common.reg_server_callback reg_server_callback,
	3:hub_call_client_create_remote_entity create_remote_entity,
	4:hub_call_client_delete_remote_entity delete_remote_entity,
	5:hub_call_client_refresh_entity refresh_entity,
	6:hub_call_client_rpc call_rpc,
	7:hub_call_client_rsp call_rsp,
	8:hub_call_client_err call_err,
	9:hub_call_client_ntf call_ntf,
	10:hub_call_client_global call_global,
	11:hub_call_kick_off_client kick_off,
	12:hub_call_kick_off_client_complete kick_off_complete,
	13:hub_call_transfer_client transfer,
	14:hub_call_transfer_entity_complete transfer_complete,
}

/*
 * ntf_client_request_login.
 */
struct client_request_hub_login {
	1:string sdk_uuid
}

/*
 * ntf_client_request_reconnect.
 */
struct client_request_hub_reconnect {
	1:string account_id,
	2:string token
}

/*
 * ntf_client_request_hub_service.
 */
struct client_request_hub_service {
	1:string service_name
}

/*
 * client send rpc msg to hub.
 */
struct client_call_hub_rpc {
	1:string entity_id,
	2:i64 msg_cb_id,
	3:common.msg message
}

/*
 * client send rsp to hub.
 */
struct client_call_hub_rsp {
	1:common.rpc_rsp rsp
}

/*
 * client send rsp err to hub.
 */
struct client_call_hub_err {
	1:common.rpc_err err
}

/*
 * client send ntf to hub.
 */
struct client_call_hub_ntf {
	1:string entity_id,
	2:common.msg message
}

/*
 * client heartbeats
 */
struct client_call_gate_heartbeats {
}

union gate_client_service {
	1:client_request_hub_login login,
	2:client_request_hub_reconnect reconnect,
	3:client_request_hub_service request_hub_service, 
	4:client_call_hub_rpc call_rpc,
	5:client_call_hub_rsp call_rsp,
	6:client_call_hub_err call_err,
	7:client_call_hub_ntf call_ntf,
	8:client_call_gate_heartbeats heartbeats
}