struct CServerEventContext {

}

struct ServerEventContext {
    
}

enum ServerEventHanderType {
    ChatEvent,
}

struct ChatEvent {

}

type ChatEventHandler = fn(ServerEventContext, ChatEvent);