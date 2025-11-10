export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
}

export const chatHistories: Record<string, ChatMessage[]> = {
  'demo-translator': [
    {
      role: 'user',
      content: "请将'你好，最近怎么样？'翻译成英文",
    },
    {
      role: 'assistant',
      content: 'Hello, how have you been recently?',
    },
    {
      role: 'user',
      content: 'What about Spanish?',
    },
    {
      role: 'assistant',
      content: 'Hola, ¿cómo has estado últimamente?',
    },
    {
      role: 'user',
      content: 'And Japanese?',
    },
    {
      role: 'assistant',
      content: 'こんにちは、最近どうですか？ (Konnichiwa, saikin dō desu ka?)',
    },
  ],

  'demo-code-reviewer': [
    {
      role: 'user',
      content: 'Review this code: `function add(a,b){return a+b}`',
    },
    {
      role: 'assistant',
      content: `I found several issues:

**Problems:**
1. Missing type annotations
2. No input validation
3. Missing JSDoc comment

**Recommended improvement:**
\`\`\`typescript
/**
 * Adds two numbers together
 * @param a First number
 * @param b Second number
 * @returns Sum of a and b
 */
function add(a: number, b: number): number {
  if (typeof a !== 'number' || typeof b !== 'number') {
    throw new TypeError('Both arguments must be numbers');
  }
  return a + b;
}
\`\`\``,
    },
    {
      role: 'user',
      content: 'What about async error handling?',
    },
    {
      role: 'assistant',
      content: `For async functions, always use try-catch blocks with proper error types:

\`\`\`typescript
async function fetchData(): Promise<Data> {
  try {
    const response = await fetch('/api/data');
    if (!response.ok) {
      throw new Error(\`HTTP error: \${response.status}\`);
    }
    return await response.json();
  } catch (error) {
    if (error instanceof TypeError) {
      throw new NetworkError('Failed to fetch');
    }
    throw error;
  }
}
\`\`\`

**Best practices:**
- Always validate response status
- Use specific error types
- Provide meaningful error messages
- Consider retry logic for network errors`,
    },
  ],

  'demo-content-creator': [
    {
      role: 'user',
      content: 'Create a tweet about AI automation',
    },
    {
      role: 'assistant',
      content:
        '🤖 AI automation is transforming how we work! From streamlining workflows to intelligent decision-making, the future is here. Are you ready to embrace the change? #AI #Automation #FutureOfWork',
    },
    {
      role: 'user',
      content: 'Make it more technical',
    },
    {
      role: 'assistant',
      content:
        '🔧 LLM orchestration + workflow automation = 10x productivity boost. RestFlow combines Claude/GPT-4 with visual workflow design for enterprise-grade automation. Built with Rust for performance. Open source! #MachineLearning #DevOps',
    },
    {
      role: 'user',
      content: 'Write a LinkedIn post version',
    },
    {
      role: 'assistant',
      content: `🚀 **Revolutionizing Enterprise Workflows with AI**

We're excited to share how AI-powered automation is transforming business operations. By combining LLMs with intelligent workflow orchestration, teams are achieving unprecedented efficiency gains.

**Key benefits:**
✅ 80% reduction in manual tasks
✅ Real-time decision automation
✅ Seamless integration with existing tools

The future of work isn't about replacing humans—it's about empowering them.

#Innovation #BusinessTransformation #AI #Productivity`,
    },
  ],

  'demo-data-analyst': [
    {
      role: 'user',
      content: 'Analyze this sales data: [120, 135, 145, 158, 170, 165, 180]',
    },
    {
      role: 'assistant',
      content: `📊 **Sales Data Analysis**

**Key Metrics:**
- Total: 1,073 units
- Average: 153.3 units/period
- Median: 158 units
- Trend: +50% growth (120 → 180)

**Insights:**
✅ Consistent upward trend (except period 6: -2.9%)
✅ Strong momentum: +15% in final period
⚠️ Minor dip in period 6 requires investigation

**Recommendation:** Maintain current strategy while analyzing period 6 anomaly.`,
    },
    {
      role: 'user',
      content: "What's the forecast for next period?",
    },
    {
      role: 'assistant',
      content: `**Forecast Analysis:**

Using linear regression model:
- **Predicted value:** ~188 units
- **Confidence interval:** 180-196 units (95%)
- **Growth rate:** +4.4% from last period

The model shows R² = 0.94, indicating high reliability.

**Recommendation:** Prepare inventory for 190 units to meet expected demand with buffer.`,
    },
  ],
}
