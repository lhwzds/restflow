import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useConfirm } from '../useConfirm'

describe('useConfirm', () => {
  beforeEach(() => {
    // Reset the resolver queue between tests
    vi.resetModules()
  })

  it('should resolve promise with true when handleConfirm is called', async () => {
    const { confirm, handleConfirm } = useConfirm()
    
    const promise = confirm({
      title: 'Test',
      description: 'Test description',
    })
    
    // Simulate user clicking confirm
    handleConfirm()
    
    const result = await promise
    expect(result).toBe(true)
  })

  it('should resolve promise with false when handleCancel is called', async () => {
    const { confirm, handleCancel } = useConfirm()
    
    const promise = confirm({
      title: 'Test',
      description: 'Test description',
    })
    
    // Simulate user clicking cancel
    handleCancel()
    
    const result = await promise
    expect(result).toBe(false)
  })

  it('should queue multiple confirm calls and resolve in order', async () => {
    const { confirm, handleConfirm, handleCancel } = useConfirm()
    
    // Call confirm twice rapidly
    const promise1 = confirm({ title: 'First', description: 'First call' })
    const promise2 = confirm({ title: 'Second', description: 'Second call' })
    
    // First user confirms
    handleConfirm()
    const result1 = await promise1
    expect(result1).toBe(true)
    
    // Second user cancels
    handleCancel()
    const result2 = await promise2
    expect(result2).toBe(false)
  })

  it('should queue multiple cancel calls and resolve in order', async () => {
    const { confirm, handleCancel } = useConfirm()
    
    const promise1 = confirm({ title: 'First', description: 'First call' })
    const promise2 = confirm({ title: 'Second', description: 'Second call' })
    
    // Both users cancel
    handleCancel()
    const result1 = await promise1
    expect(result1).toBe(false)
    
    handleCancel()
    const result2 = await promise2
    expect(result2).toBe(false)
  })

  it('should handle rapid confirm calls correctly', async () => {
    const { confirm, handleConfirm } = useConfirm()
    
    // Fire 5 rapid confirm calls
    const promises = Array.from({ length: 5 }, () => 
      confirm({ title: 'Rapid', description: 'Rapid call' })
    )
    
    // Resolve all in order
    for (const promise of promises) {
      handleConfirm()
      const result = await promise
      expect(result).toBe(true)
    }
  })

  it('should use default confirm/cancel text when not provided', () => {
    const { confirm, options, handleConfirm } = useConfirm()

    // Don't await - just check options were set synchronously
    confirm({
      title: 'Test',
      description: 'Test description',
    })

    expect(options.value.confirmText).toBe('Confirm')
    expect(options.value.cancelText).toBe('Cancel')
    // Clean up queued resolver
    handleConfirm()
  })

  it('should use custom confirm/cancel text when provided', () => {
    const { confirm, options, handleConfirm } = useConfirm()

    confirm({
      title: 'Test',
      description: 'Test description',
      confirmText: 'Yes',
      cancelText: 'No',
    })

    expect(options.value.confirmText).toBe('Yes')
    expect(options.value.cancelText).toBe('No')
    handleConfirm()
  })

  it('should use default variant when not provided', () => {
    const { confirm, options, handleConfirm } = useConfirm()

    confirm({
      title: 'Test',
      description: 'Test description',
    })

    expect(options.value.variant).toBe('default')
    handleConfirm()
  })

  it('should use custom variant when provided', () => {
    const { confirm, options, handleConfirm } = useConfirm()

    confirm({
      title: 'Test',
      description: 'Test description',
      variant: 'destructive',
    })

    expect(options.value.variant).toBe('destructive')
    handleConfirm()
  })
})
