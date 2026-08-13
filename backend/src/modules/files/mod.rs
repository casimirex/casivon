//! Uploaded files.
//!
//! Deliberately its own module rather than part of HR. A receipt is the first
//! thing anybody uploads, but it is not an HR concept — an invoice with a
//! signed delivery note or a purchase order with a supplier quotation want the
//! same two endpoints, and would want them again if this lived under
//! `/hr/expense-reports/{id}/receipt`.

pub mod application;
pub mod domain;
pub mod handlers;
pub mod infrastructure;
pub mod routes;
