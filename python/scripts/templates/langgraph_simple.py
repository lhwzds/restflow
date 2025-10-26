# /// script
# dependencies = [
#   "langgraph>=1.0",
#   "langchain>=1.0",
# ]
# ///

import json
import sys
from typing import TypedDict
from langgraph.graph import StateGraph, END

# Define your state
class GraphState(TypedDict):
    messages: list[str]
    result: str

# Define nodes
def process_node(state: GraphState):
    messages = state.get("messages", [])
    result = f"Processed {len(messages)} messages"
    return {"result": result}

# Build the graph
graph = StateGraph(GraphState)
graph.add_node("process", process_node)
graph.set_entry_point("process")
graph.set_finish_point("process")

# Compile
compiled_graph = graph.compile()

# Execute
if __name__ == "__main__":
    input_data = json.load(sys.stdin)
    result = compiled_graph.invoke(input_data)
    print(json.dumps(result))
