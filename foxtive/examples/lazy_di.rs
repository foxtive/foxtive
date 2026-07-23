//! Example demonstrating circular dependency resolution with `Lazy<T>`.
//!
//! Shows both `#[derive(Service)]` and manual `ServiceInit` approaches.

use foxtive::container::Lazy;
use foxtive::lifecycle::{Service, ServiceInit};
use foxtive::prelude::*;

#[derive(Service, Default)]
struct OrderService {
    #[dependency]
    payment: Lazy<PaymentService>,
}

impl OrderService {
    fn process_order(&self, order_id: &str) -> String {
        let payment_info = self.payment.process_payment(order_id, 99.99);
        format!("Order {order_id} processed: {payment_info}")
    }
}

#[derive(Service, Default)]
struct PaymentService {
    #[dependency]
    order: Lazy<OrderService>,
}

impl PaymentService {
    fn process_payment(&self, order_id: &str, amount: f64) -> String {
        format!("Payment of ${amount:.2} for order {order_id}")
    }

    fn refund(&self, order_id: &str) -> String {
        let _order = &*self.order;
        format!("Refund issued for order {order_id}")
    }
}

/// Inventory service with manual ServiceInit and custom wire_lazy.
struct InventoryService {
    shipping: Lazy<ShippingService>,
}

impl ServiceInit for InventoryService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self {
            shipping: lazy!(), // wire_lazy fills this
        })
    }

    fn wire_lazy(app: &App) -> AppResult<()> {
        let svc = app.require::<Self>()?;
        app.require_lazy::<ShippingService>(&svc.shipping)?;
        Ok(())
    }
}

impl InventoryService {
    fn check_stock(&self, item: &str) -> String {
        let _shipping_info = self.shipping.estimate_delivery(item);
        format!("Item '{item}' is in stock")
    }
}

/// Shipping service that depends on InventoryService.
struct ShippingService {
    inventory: Lazy<InventoryService>,
}

impl ServiceInit for ShippingService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self {
            inventory: lazy!(), // wire_lazy fills this
        })
    }

    fn wire_lazy(app: &App) -> AppResult<()> {
        let svc = app.require::<Self>()?;
        app.require_lazy::<InventoryService>(&svc.inventory)?;
        Ok(())
    }
}

impl ShippingService {
    fn estimate_delivery(&self, item: &str) -> String {
        let _stock = &*self.inventory;
        format!("Delivery for '{item}': 3-5 business days")
    }

    fn ship(&self, item: &str) -> String {
        format!("Shipped '{item}' via express")
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    println!("=== Lazy<T> - #[derive(Service)] (zero boilerplate) ===\n");

    let app = App::builder("Order System", "ORDSYS")
        .register_service::<OrderService>()
        .register_service::<PaymentService>()
        .build()
        .await?;

    let order_svc = app.require::<OrderService>()?;
    let payment_svc = app.require::<PaymentService>()?;

    println!("{}", order_svc.process_order("ORD-001"));
    println!("{}", payment_svc.refund("ORD-002"));
    println!(
        "OrderService.payment filled: {}",
        order_svc.payment.is_filled()
    );
    println!(
        "PaymentService.order filled:   {}",
        payment_svc.order.is_filled()
    );

    println!("\n=== Lazy<T> - Manual ServiceInit (custom init) ===\n");

    let app2 = App::builder("Warehouse System", "WAREHS")
        .register_service::<InventoryService>()
        .register_service::<ShippingService>()
        .build()
        .await?;

    let inventory = app2.require::<InventoryService>()?;
    let shipping = app2.require::<ShippingService>()?;

    println!("{}", inventory.check_stock("widget"));
    println!("{}", shipping.ship("widget"));
    println!(
        "InventoryService.shipping filled:  {}",
        inventory.shipping.is_filled()
    );
    println!(
        "ShippingService.inventory filled:  {}",
        shipping.inventory.is_filled()
    );

    println!("\n=== Example complete ===");
    Ok(())
}
