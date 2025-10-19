export interface ApiResponse<T> {
  success: boolean
  data?: T
  message?: string
}

export interface TestWorkflowResponse {
  execution_id: string
}
