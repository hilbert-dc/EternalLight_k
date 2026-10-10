import test from "node:test";
import assert from "node:assert/strict";
import {TaskBudget} from "../src/state/budget.mjs";
test("trusted logical media reservations count once and conflict on changed token bounds",()=>{
  const budget=new TaskBudget({maxRequests:1,maxTokens:10});
  const parameters={reservationId:"media:operation",estimatedInputTokens:0,maxOutputTokens:0};
  const id=budget.reserveRequest(parameters);assert.equal(id,"media:operation");
  assert.equal(budget.reserveRequest(parameters),id);assert.equal(budget.snapshot().requests,1);
  assert.throws(()=>budget.reserveRequest({...parameters,maxOutputTokens:1}),/parameter conflict/);
  budget.settleReservation(id,null,{outcome:"unknown"});assert.equal(budget.snapshot().requests,1);
  assert.equal(budget.reserveRequest(parameters),id);assert.equal(budget.snapshot().requests,1);
  assert.throws(()=>budget.reserveRequest({...parameters,reservationId:"media:another"}),/budget exhausted/);
});
