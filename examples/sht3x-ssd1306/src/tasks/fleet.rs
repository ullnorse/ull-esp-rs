use embassy_net::Stack;
use ull_esp_platform::fleet::Agent;

use crate::app::METRICS;

#[embassy_executor::task]
pub async fn fleet_task(agent: Agent, stack: Stack<'static>) {
    agent.run(stack, &METRICS).await
}
