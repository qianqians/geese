include "common.thrift"

/*
 * gate forward hub msg to client.
 * create remote entity in client.
 */
struct create_remote_entity {
	1:string entity_id,
	2:string entity_type,
	3:bool is_main,
	4:binary argvs
}

/*
 * hub command delete entity
 */
struct delete_remote_entity {
	1:string entity_id
}

/*
 * hub command refresh remote entity.
 */
struct refresh_entity {
	1:string entity_id,
	2:string entity_type,
	3:bool is_main,
	4:binary argvs
}

/*
 * gate ntf client reconnect server complete 
 */
struct transfer_complete {
}

/*
 * gate forward hub msg kick_off client
 */
struct kick_off {
	1:string prompt_info
}

/*
 * gate ntf client conn_id
 */
struct ntf_conn_id {
	1:string conn_id
}

/*
 * gate forward hub call rpc to client.
 */
struct call_rpc {
	1:string hub_name,
	2:string entity_id,
	3:i64 msg_cb_id,
	4:common.msg message
}

/*
 * gate forward hub callback rsp to client.
 */
struct call_rsp {
	1:common.rpc_rsp rsp
}

/*
 * gate forward hub callback err to client.
 */
struct call_err {
	1:common.rpc_err err
}

/*
 * gate forward hub send ntf msg to client.
 */
struct call_ntf {
	1:string hub_name,
	2:string entity_id,
	3:common.msg message
}

/*
 * gate forward hub send global msg to client.
 */
struct call_global {
	1:string hub_name,
	2:common.msg message
}

/*
 * gate call heartbeats
 */
struct gate_call_heartbeats {
	1:i64 timetmp
}

union client_service {
	1:create_remote_entity create_remote_entity,
	2:delete_remote_entity delete_remote_entity,
	3:refresh_entity refresh_entity,
	4:ntf_conn_id conn_id,
	5:kick_off kick_off,
	6:transfer_complete transfer_complete,
	7:call_rpc call_rpc,
	8:call_rsp call_rsp,
	9:call_err call_err,
	10:call_ntf call_ntf,
	11:call_global call_global,
	12:gate_call_heartbeats heartbeats
}