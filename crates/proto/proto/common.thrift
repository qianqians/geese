
struct msg {
	1:string method,
	2:binary argvs
}

struct rpc_rsp {
	1:string entity_id,
	2:i64 msg_cb_id,
	3:binary argvs
}

struct rpc_err {
	1:string entity_id,
	2:i64 msg_cb_id,
	3:binary argvs
}


struct redis_msg {
	1:string server_name,
	2:binary msg
}

/*
 * register server to other server.
 */
struct reg_server {
	1:string name,
	2:string type
}

/*
 * register server to other server callback.
 */
struct reg_server_callback {
	1:string name
}
