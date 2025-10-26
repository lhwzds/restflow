# /// script
# dependencies = [
#   "langgraph>=1.0",
#   "langchain>=1.0",
#   "langchain-openai>=0.1.0",
# ]
# ///

# /// metadata
# {
#   "name": "LangGraph - ReAct Agent",
#   "description": "ReAct agent with OpenAI integration and message handling"
# }
# ///

import json
import sys
from typing import TypedDict, Annotated
from langgraph.graph import StateGraph, END
from langchain_openai import ChatOpenAI
from langchain.schema import HumanMessage, AIMessage

# Define agent state
class AgentState(TypedDict):
    messages: Annotated[list, "Message history"]
    next_action: str

# Initialize LLM
llm = ChatOpenAI(model="gpt-4", temperature=0)

def agent_node(state: AgentState):
    """ReAct agent that processes messages and decides next action"""
    messages = state.get("messages", [])

    # Convert to LangChain message objects
    lc_messages = []
    for msg in messages:
        if isinstance(msg, dict):
            if msg.get("role") == "user":
                lc_messages.append(HumanMessage(content=msg.get("content", "")))
            elif msg.get("role") == "assistant":
                lc_messages.append(AIMessage(content=msg.get("content", "")))
        elif isinstance(msg, str):
            lc_messages.append(HumanMessage(content=msg))

    # Get LLM response
    response = llm.invoke(lc_messages)

    # Update state
    return {
        "messages": messages + [{"role": "assistant", "content": response.content}],
        "next_action": "complete"
    }

def should_continue(state: AgentState):
    """Determine if agent should continue or end"""
    return state.get("next_action", "complete")

# Build the graph
graph = StateGraph(AgentState)
graph.add_node("agent", agent_node)
graph.set_entry_point("agent")

# Add conditional edge
graph.add_conditional_edges(
    "agent",
    should_continue,
    {
        "complete": END,
        "continue": "agent"
    }
)

# Compile
compiled_graph = graph.compile()

# Execute
if __name__ == "__main__":
    input_data = json.load(sys.stdin)
    result = compiled_graph.invoke(input_data)
    print(json.dumps(result))
