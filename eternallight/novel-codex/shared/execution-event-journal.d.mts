export interface ExecutionEvent {
  method: string;
  params?: Record<string, any>;
  hostExecution: {id:string;requestId:string;projectId:string;threadId:string|null;sequence:number};
}
export class ExecutionEventJournal {
  constructor(options?: {compact?:boolean});
  cursor:number;
  startTurn():void;
  append(event:ExecutionEvent):void;
  snapshot():{cursor:number;events:ExecutionEvent[];truncated:boolean};
  after(cursor:number):ExecutionEvent[];
}
