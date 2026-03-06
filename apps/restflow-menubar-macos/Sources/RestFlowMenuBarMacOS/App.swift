import SwiftUI
import AppKit
import Combine

@main
struct RestFlowMenuBarApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        Settings {
            EmptyView()
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let stateModel = PollingStateModel()
    private var cancellables = Set<AnyCancellable>()
    private var statusItem: NSStatusItem?
    private let popover = NSPopover()

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupStatusItem()
        setupPopover()
        bindState()
        stateModel.start()
    }

    func applicationWillTerminate(_ notification: Notification) {
        stateModel.stop()
    }

    private func setupStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.title = "RF"
        item.button?.toolTip = "RestFlow Menu Bar"
        item.button?.target = self
        item.button?.action = #selector(togglePopover)
        statusItem = item
    }

    private func setupPopover() {
        let contentView = MenuPopoverRootView(stateModel: stateModel)
        let host = NSHostingController(rootView: contentView)

        popover.contentSize = NSSize(width: 660, height: 440)
        popover.behavior = .transient
        popover.animates = true
        popover.contentViewController = host
    }

    private func bindState() {
        stateModel.$snapshot
            .receive(on: DispatchQueue.main)
            .sink { [weak self] snapshot in
                guard let self else { return }
                guard let snapshot else {
                    self.statusItem?.button?.title = "RF"
                    return
                }

                let status = snapshot.daemon.status
                let activeTasks = snapshot.summary.tasks.active
                let indicator: String
                switch status {
                case "running":
                    indicator = "●"
                case "stale":
                    indicator = "!"
                default:
                    indicator = "○"
                }
                self.statusItem?.button?.title = activeTasks > 0
                    ? "RF\(activeTasks)\(indicator)"
                    : "RF\(indicator)"
            }
            .store(in: &cancellables)
    }

    @objc private func togglePopover() {
        guard let button = statusItem?.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            popover.contentViewController?.view.window?.becomeKey()
        }
    }
}
