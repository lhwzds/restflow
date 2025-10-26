# /// script
# dependencies = [
#   "langgraph>=1.0",
#   "langchain>=1.0",
#   "langchain-openai>=0.1.0",
# ]
# ///

# /// metadata
# {
#   "name": "LangGraph - Multi-Agent System",
#   "description": "Collaborative multi-agent system with researcher and writer agents"
# }
# ///

import json
import sys
from typing import TypedDict, Annotated, Literal
from langgraph.graph import StateGraph, END
from langchain_openai import ChatOpenAI

# Define multi-agent state
class MultiAgentState(TypedDict):
    task: str
    research_output: str
    writer_output: str
    final_output: str
    current_agent: str

# Initialize LLMs with different roles (API key loaded from OPENAI_API_KEY environment variable)
researcher = ChatOpenAI(model="gpt-4.1", temperature=0.3)
writer = ChatOpenAI(model="gpt-4.1", temperature=0.7)

def research_agent(state: MultiAgentState):
    """Research agent that gathers information"""
    task = state.get("task", "")

    prompt = f"""You are a research agent. Analyze this task and provide key information:
Task: {task}

Provide a structured summary of important facts and context."""

    response = researcher.invoke([{"role": "user", "content": prompt}])

    return {
        "research_output": response.content,
        "current_agent": "writer"
    }

def writer_agent(state: MultiAgentState):
    """Writer agent that creates final output"""
    task = state.get("task", "")
    research = state.get("research_output", "")

    prompt = f"""You are a writer agent. Using the research below, create a well-structured response:

Original Task: {task}

Research: {research}

Write a clear, concise response."""

    response = writer.invoke([{"role": "user", "content": prompt}])

    return {
        "writer_output": response.content,
        "final_output": response.content,
        "current_agent": "complete"
    }

def route_agent(state: MultiAgentState) -> Literal["research", "writer", "complete"]:
    """Route to next agent based on current state"""
    current = state.get("current_agent", "research")

    if current == "research":
        return "research"
    elif current == "writer":
        return "writer"
    else:
        return "complete"

# Build multi-agent graph
graph = StateGraph(MultiAgentState)

# Add agent nodes
graph.add_node("research", research_agent)
graph.add_node("writer", writer_agent)

# Set entry point
graph.set_entry_point("research")

# Add edges
graph.add_edge("research", "writer")
graph.add_edge("writer", END)

# Compile
compiled_graph = graph.compile()

# Execute
if __name__ == "__main__":
    try:
        input_data = json.load(sys.stdin)
    except:
        input_data = {}

    # Ensure we have a task field with default value
    if not input_data.get("task"):
        input_data["task"] = "Explain the benefits of using LangGraph for building multi-agent systems"

    result = compiled_graph.invoke(input_data)
    print(json.dumps(result))
